//! Travel: carrying a journey out for the player's character.
//!
//! A journey is planned over walking, the portals this world's store knows and the character's
//! own recalls ([`dereth_navigation::plan_journey`]), and each leg is carried out as a player
//! would: walking with the turn and forward keys (a long outdoor leg a window at a time, round the
//! obstacles and the creatures standing near the route, opening doors on the way), using a
//! portal once standing at it, recalling by item, by spell (taking up a caster and going into
//! the magic stance first) or by command, and handing a person's token back to them (the Training
//! Academy's way out, which the player is asked about first: it is for good). What each use shows
//! is learned and kept per world: where a portal sent the character, that it refused, where each
//! recall landed. A leg that fails is learned from and the journey planned again.
//!
//! One journey runs at a time. It is asked for with a
//! [`TravelRequest`](crate::travel::TravelRequest) and answers with
//! [`TravelEvent`](crate::travel::TravelEvent)s; its state for display is a
//! [`TripView`](crate::travel::TripView). The player's own movement ends it.
//!
//! The store is one text file per world, in the folder the travel was given, beside an optional
//! hints file read at each start and never written.
//!
//! Planning is never done in the frame: journeys and routes are asked of [`crate::planning`] and
//! taken up when they are answered, some frames later; the character stands while it waits.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dereth_dat::RetailDatStore;
use dereth_navigation::{
    global, global_distance, CharacterState, DoorState, EdgeSource, FollowConfig, Follower,
    HandBack, Journey, LandNavSource, Leg, NavConfig, NavPath, NavWorld, NoPath, Obstacle,
    ObstacleKind, PortalSighting, PortalStore, Recall, StaticPlacement, Steer, Teleport,
    TeleportOutcome, TeleportWatch, WaypointAction,
};
use dereth_physics::SetupGeometry;
use dereth_primitives::num::math::hypotf;
use dereth_primitives::num::to_i32;
use dereth_primitives::{CellId, DataId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};
use dereth_world_data::land_source::DatLandSource;

use crate::planning::{Answer, Goal, Job, Planning};

/// A setup resolver over the dat store, memoised.
pub type Setups = Box<dyn Fn(DataId) -> Option<Arc<SetupGeometry>>>;

/// The outdoor placements of a landblock, memoised.
pub type Placements = Box<dyn Fn(LandblockId) -> Vec<StaticPlacement>>;

/// The planner over the client's own land source.
pub type Planner = NavWorld<LandNavSource<Arc<DatLandSource>, Setups, Placements>>;

/// Which kinds of leg a journey may use besides walking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelAllow {
    /// Portals.
    pub portals: bool,
    /// Recall spells the character casts.
    pub spells: bool,
    /// Recall items the character carries.
    pub items: bool,
    /// The recalls every character has by command.
    pub commands: bool,
}

impl Default for TravelAllow {
    fn default() -> Self {
        Self {
            portals: true,
            spells: true,
            items: true,
            commands: true,
        }
    }
}

/// Where a journey goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TravelTarget {
    /// A place.
    Place(Position),
    /// An object, where it is when the journey is planned.
    Object(ObjectId),
    /// A cell: an interior cell's middle, or a land cell's middle on the ground.
    Cell(CellId),
}

/// What is asked of the travel.
#[derive(Debug, Clone, PartialEq)]
pub enum TravelRequest {
    /// Go there: a journey already running ends `replaced`.
    Go {
        id: u64,
        target: TravelTarget,
        allow: TravelAllow,
    },
    /// The journey there, planned but not carried out.
    Plan {
        id: u64,
        target: TravelTarget,
        allow: TravelAllow,
    },
    /// Use each recall whose destination is not yet known, to learn it.
    LearnRecalls { id: u64 },
    /// The walking route there, round what stands in the way and through doors, planned but not
    /// walked.
    Route { id: u64, target: TravelTarget },
    /// End journey `id` with `reason`.
    Cancel { id: u64, reason: String },
}

/// One leg of a journey, as shown and told.
#[derive(Debug, Clone, PartialEq)]
pub enum TravelLeg {
    /// Walk to `to`; `path` the route's corners once it is planned (empty before).
    Walk { to: Position, path: Vec<Position> },
    /// Open this door on the way.
    Door { object: ObjectId, at: Position },
    /// Use this portal.
    Portal {
        name: String,
        wcid: Option<u32>,
        from: Position,
        to: Option<Position>,
        source: EdgeSource,
    },
    /// Recall.
    Recall {
        name: String,
        recall: Recall,
        item: Option<ObjectId>,
        to: Position,
    },
    /// Hand the token back to the person `name` standing at `at` (taking it from them first),
    /// and be sent on to `to`.
    HandBack {
        name: String,
        token: String,
        at: Position,
        to: Position,
    },
}

impl TravelLeg {
    /// The leg's kind, as a word.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Walk { .. } => "walk",
            Self::Door { .. } => "door",
            Self::Portal { .. } => "portal",
            Self::Recall { .. } => "recall",
            Self::HandBack { .. } => "hand_back",
        }
    }

    /// Where the leg ends, when it is known.
    #[must_use]
    pub fn end(&self) -> Option<Position> {
        match self {
            Self::Walk { to, .. } => Some(*to),
            Self::Door { at, .. } => Some(*at),
            Self::Portal { to, .. } => *to,
            Self::Recall { to, .. } | Self::HandBack { to, .. } => Some(*to),
        }
    }

    fn of(leg: &Leg) -> Self {
        match leg {
            Leg::Walk { to } => Self::Walk {
                to: *to,
                path: Vec::new(),
            },
            Leg::Portal { edge } => Self::Portal {
                name: edge.name.clone(),
                wcid: edge.wcid,
                from: edge.from,
                to: edge.to,
                source: edge.source,
            },
            Leg::Teleport { teleport } => Self::Recall {
                name: teleport.name.clone(),
                recall: teleport.recall,
                item: teleport.item,
                to: teleport.to,
            },
            Leg::HandBack { hand_back } => Self::HandBack {
                name: hand_back.name.clone(),
                token: hand_back.token.clone(),
                at: hand_back.at,
                to: hand_back.to,
            },
        }
    }
}

/// A planned journey, as shown and told.
#[derive(Debug, Clone, PartialEq)]
pub struct TravelPlan {
    /// The legs in order; the first walk's doors as legs of their own when its route is planned.
    pub legs: Vec<TravelLeg>,
    /// Seconds it is expected to take.
    pub seconds: f32,
}

/// A walking route, planned but not walked: the client's own planner's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct TravelRoute {
    /// The corners: the first where the character stands, the last the place to stand nearest the
    /// target.
    pub points: Vec<Position>,
    /// The closed doors on the way: each door and the corner (an index into `points`) it is
    /// opened from before the next leg walks through it.
    pub doors: Vec<(ObjectId, usize)>,
    /// Metres along the corners.
    pub length: f32,
    /// Whether the route reaches the target: false when the target lies beyond what one plan
    /// covers, the route then ending on the way there.
    pub complete: bool,
}

/// What a use taught.
#[derive(Debug, Clone, PartialEq)]
pub enum Learned {
    /// A portal sent the character here.
    Portal { name: String, to: Position },
    /// A portal refused the character.
    Refusal { name: String, reason: String },
    /// A recall sent the character here.
    Recall { recall: Recall, to: Position },
}

/// What a journey came to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TripStats {
    /// Journeys planned (the first, and one for each replan after a failed leg).
    pub journeys: u32,
    pub portal_uses: u32,
    pub refusals: u32,
    pub recalls: u32,
    /// Metres walked.
    pub walked: f32,
    /// Seconds by the game clock and the wall clock.
    pub game_seconds: f64,
    pub wall_seconds: f64,
    pub stuck: u32,
    pub replans: u32,
    /// Replans round obstacles that appeared, and round creatures.
    pub obstacle_replans: u32,
    pub creature_replans: u32,
    pub doors: u32,
}

/// What happened to a journey.
#[derive(Debug, Clone, PartialEq)]
pub enum TravelEvent {
    /// A journey was planned (`reason` why, when it replaces one that failed).
    Planned {
        id: u64,
        plan: TravelPlan,
        reason: Option<String>,
    },
    /// The answer to [`TravelRequest::Plan`]: the plan, or why there is none.
    Answer {
        id: u64,
        plan: Result<TravelPlan, String>,
    },
    /// The answer to [`TravelRequest::Route`]: the route, or why there is none (`no_position`,
    /// `no_world`, `no_target`, `no_route`, `too_far`).
    Route {
        id: u64,
        route: Result<TravelRoute, String>,
    },
    /// Leg `index` began.
    Leg {
        id: u64,
        index: usize,
        leg: TravelLeg,
    },
    /// A use taught something.
    Learned { id: u64, what: Learned },
    /// The journey arrived, `distance` metres from the goal.
    Arrived {
        id: u64,
        distance: f32,
        stats: TripStats,
    },
    /// The journey ended without arriving.
    Failed {
        id: u64,
        reason: String,
        stats: TripStats,
    },
}

/// The journey in progress, for display.
#[derive(Debug, Clone, PartialEq)]
pub struct TripView {
    pub id: u64,
    /// Its legs.
    pub legs: Vec<TravelLeg>,
    /// The leg being carried out, an index into `legs`.
    pub leg: usize,
    /// Where the leg being carried out goes: the walk's end, the portal, a recall's landing.
    pub target: Option<Position>,
    /// The way still to go: the rest of the walk being followed, then each later leg's end.
    pub route: Vec<Position>,
    /// The goal.
    pub goal: Position,
}

/// What the travel tells a frame: this frame's events and the journey in progress.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TravelStatus {
    pub events: Vec<TravelEvent>,
    pub trip: Option<TripView>,
}

/// What the travel asks the frame to do.
#[derive(Debug, Clone, PartialEq)]
pub enum TravelCommand {
    /// Hold or release the forward key.
    Forward(bool),
    /// Hold the turn-left key (`Some(false)`), the turn-right key (`Some(true)`), or neither.
    Turn(Option<bool>),
    /// Use this object: a door, a portal, a recall item.
    Use(ObjectId),
    /// Take up this item (a caster).
    Wield(ObjectId),
    /// Go into or out of the combat stance the wielded item gives.
    ToggleCombat,
    /// Cast this spell.
    Cast(u32),
    /// Send this chat line, as typed.
    Chat(String),
    /// Give this item, one of the character's, to this person.
    Give { item: ObjectId, target: ObjectId },
    /// Ask the player this, yes or no; the answer comes back as [`TravelInput::answer`].
    Ask(String),
}

/// An object in the world a route has to respect, with its name for the log.
#[derive(Debug, Clone)]
pub struct WorldObstacle {
    pub obstacle: Obstacle,
    pub name: String,
}

/// A portal the character can see, with its object id.
#[derive(Debug, Clone)]
pub struct PortalSeen {
    pub id: ObjectId,
    pub sighting: PortalSighting,
}

/// A person in sight who sends the character on when it hands back the token they give it (the
/// Training Academy's exit person), with the question the player is asked before a journey hands
/// it back. Where they send the character is the travel's to know: where a portal of their
/// dungeon that leads out of it sends.
#[derive(Debug, Clone, PartialEq)]
pub struct HandBackSeen {
    pub object: ObjectId,
    pub name: String,
    pub at: Position,
    /// The name of the token they give and take back.
    pub token: String,
    pub question: String,
}

/// The character's stance and caster, for casting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stance {
    /// In the magic stance now.
    pub magic: bool,
    /// A caster is wielded.
    pub caster_wielded: bool,
    /// A caster carried in the pack, to take up.
    pub caster_carried: Option<ObjectId>,
    /// Busy with something else (a use, a cast): a cast waits.
    pub busy: bool,
}

/// What the frame tells the travel.
pub struct TravelInput<'a> {
    /// The game clock the body is stepped by.
    pub now: f64,
    /// The wall clock.
    pub wall: f64,
    /// The player's position, when there is a body.
    pub at: Option<Position>,
    /// Whether no teleport is in progress.
    pub idle: bool,
    /// The land source, when the world is built.
    pub land: Option<&'a Arc<DatLandSource>>,
    /// The dat store.
    pub store: &'a Arc<RetailDatStore>,
    /// The character's recalls and state.
    pub character: CharacterState,
    /// Its stance and caster.
    pub stance: Stance,
    /// The portals in sight.
    pub portals: Vec<PortalSeen>,
    /// The world's identity, once known.
    pub world: Option<String>,
    /// Whether the player moved the character themselves this frame.
    pub player_input: bool,
    /// Whether a spell the character cast fizzled since the last step.
    pub fizzled: bool,
    /// Where an object is.
    pub object: &'a dyn Fn(ObjectId) -> Option<Position>,
    /// The people in sight who send the character on for a token handed back.
    pub hand_backs: Vec<HandBackSeen>,
    /// An item of this name the character holds, if any.
    pub holds: &'a dyn Fn(&str) -> Option<ObjectId>,
    /// The player's answer to the travel's question, since the last step.
    pub answer: Option<bool>,
}

impl std::fmt::Debug for TravelInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TravelInput")
            .field("at", &self.at)
            .field("idle", &self.idle)
            .field("world", &self.world)
            .finish_non_exhaustive()
    }
}

/// Where the store's files are: `<dir>/<prefix><world>.tsv` and `<dir>/<prefix>hints.tsv`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreFiles {
    pub dir: Option<PathBuf>,
    pub prefix: String,
}

/// How long a used door is waited for to stand open, seconds; and how many uses it gets before
/// the walk goes round it for now. A door is judged locked only on the server's answer.
const DOOR_WAIT: f64 = 4.0;
const DOOR_TRIES: u32 = 4;

/// What the walk does at a closed door its route passes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtDoor {
    /// It stands open: on through it.
    Through,
    /// Use it, or use it again.
    Use,
    /// Wait for the use to take.
    Wait,
    /// The server answered a use as locked: round it.
    Locked,
    /// Used [`DOOR_TRIES`] times and still shut, with no word that it is locked (busy, or
    /// something standing in it): round it for now.
    Blocked,
}

/// At a door: `open` when it stands open; `locked` when the server answered a use of it as
/// locked; `used` when it was last used (wall seconds) and how many times; `wall` now.
fn at_door(open: bool, locked: bool, used: Option<(f64, u32)>, wall: f64) -> AtDoor {
    if open {
        return AtDoor::Through;
    }
    if locked {
        return AtDoor::Locked;
    }
    match used {
        None => AtDoor::Use,
        Some((since, _)) if wall - since <= DOOR_WAIT => AtDoor::Wait,
        Some((_, tries)) if tries < DOOR_TRIES => AtDoor::Use,
        Some(_) => AtDoor::Blocked,
    }
}

/// The step of the leg being carried out.
#[derive(Debug, Clone)]
enum Step {
    /// Waiting for the journey to be planned (job `job`); `reason` why it is planned, when it
    /// replaces one that failed.
    Planning { job: u64, reason: Option<String> },
    /// Walking toward `target`; `last` when the walk ends there rather than at a window's edge.
    Walk { target: Position, last: bool },
    /// Walking to the portal of the current leg.
    Approach { portal: Position },
    /// Getting ready to cast `spell`: taking up a caster, going into the magic stance.
    Ready {
        recall: Recall,
        since: f64,
        wielded: bool,
        toggled: bool,
    },
    /// Waiting for a teleport to land.
    Wait { what: Awaited, watch: TeleportWatch },
    /// Waiting for the player's answer to whether the journey may hand a token back (and so
    /// leave for good).
    Ask,
    /// At a hand-back: walking to the person, then taking the token and giving it back.
    HandBack {
        hand_back: HandBack,
        tries: HandBackTries,
    },
}

/// How a hand-back stands: when the person was last used for the token and how often, when the
/// token came, and when it was last given back and how often.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct HandBackTries {
    used: Option<(f64, u32)>,
    held_since: Option<f64>,
    gave: Option<(f64, u32)>,
}

/// What a hand-back does at its person this step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandBackMove {
    /// Nothing yet: the token is still to come, or the person still talking.
    Wait,
    /// Use the person, for the token (the `n`th use).
    Use(u32),
    /// Give the token back (the `n`th give).
    Give(ObjectId, u32),
    /// Given back and gone from the pack: the person took it.
    Taken,
    /// No token came in this many uses.
    NoToken(u32),
    /// The person did not take it back in this many gives.
    NotTaken(u32),
}

impl HandBackTries {
    /// Whether anything has been done at the person yet.
    const fn begun(&self) -> bool {
        self.used.is_some() || self.gave.is_some()
    }

    /// The move to make now (wall seconds `now`), the character holding `token` (or no token),
    /// the tries kept for the next step.
    fn next(&mut self, token: Option<ObjectId>, now: f64) -> HandBackMove {
        if let Some(token) = token {
            let since = *self.held_since.get_or_insert(now);
            let due = self.gave.is_none_or(|(at, _)| now - at >= GIVE_WAIT);
            if now - since < TOKEN_SETTLE || !due {
                return HandBackMove::Wait;
            }
            let n = self.gave.map_or(1, |(_, n)| n + 1);
            if n > GIVES {
                return HandBackMove::NotTaken(GIVES);
            }
            self.gave = Some((now, n));
            return HandBackMove::Give(token, n);
        }
        if self.gave.is_some() {
            return HandBackMove::Taken;
        }
        match self.used {
            Some((since, _)) if now - since < TOKEN_WAIT => HandBackMove::Wait,
            Some((_, n)) if n >= TOKEN_USES => HandBackMove::NoToken(n),
            _ => {
                let n = self.used.map_or(1, |(_, n)| n + 1);
                self.used = Some((now, n));
                HandBackMove::Use(n)
            }
        }
    }
}

/// What a teleport being waited for came from.
#[derive(Debug, Clone)]
enum Awaited {
    Portal(PortalSighting),
    Recall(Recall),
    /// A known portal the character walked into on the way, before reaching the leg that uses
    /// it.
    WalkedInto(PortalSighting),
    /// Something else took the character away (the server, or a portal the store does not
    /// know).
    Elsewhere,
    /// The person a token was handed back to, sending the character on.
    HandBack(String),
}

/// What a trip is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// Going to the goal.
    Go,
    /// Learning the recalls not yet known.
    LearnRecalls,
}

/// A walking route as planned: the route and whether it goes the whole way, or why there is none.
type Walked = Result<(NavPath, bool), NoPath>;

/// A journey in progress.
#[derive(Debug)]
struct Trip {
    id: u64,
    purpose: Purpose,
    target: TravelTarget,
    goal: Option<Position>,
    allow: TravelAllow,
    journey: Option<Journey>,
    /// The legs as shown, the first walk's doors among them.
    view: Vec<TravelLeg>,
    leg: usize,
    step: Option<Step>,
    follower: Option<Follower>,
    /// The door being opened: when it was last used (wall seconds) and how many times.
    door: Option<(ObjectId, f64, u32)>,
    /// Doors planned round on this trip: those the server said are locked, and those that did not
    /// open after [`DOOR_TRIES`] uses (something in the way, or slow), which are tried again when
    /// no other way remains.
    locked: BTreeSet<u32>,
    /// Doors whose use the server answered as locked (`OpenFailDueToLock`).
    lock_answers: BTreeSet<u32>,
    /// Recalls that did not land on this trip: not offered again.
    disabled: BTreeSet<Recall>,
    /// Something solid appeared near the route since it was planned.
    route_dirty: Option<String>,
    stats: TripStats,
    started: f64,
    started_wall: f64,
    last: Option<Position>,
    /// The stance to go back to after a cast landed.
    leave_stance: bool,
    /// Casts of the recall being waited for.
    casts: u32,
    /// The player said yes to handing a token back on this trip.
    confirmed: bool,
    /// The route the walk waits for: the job, and where it was asked toward.
    walking: Option<(u64, Position)>,
    /// The places a stopped walk took out were put back for this walk: no route even so.
    unblocked: bool,
    /// The route on from the end of the one being followed, which ends at a window's edge,
    /// asked for before the body gets there: the job, and its answer once it has come.
    ahead: Option<(u64, Option<Walked>)>,
}

/// How many times a journey is planned again before it gives up.
const MAX_REPLANS: u32 = 8;
/// A creature farther than this from the body is not planned round: it will have moved.
const CREATURE_REACH: f32 = 20.0;
/// A creature moving faster than this (metres per second of game time) is not planned round.
const CREATURE_SLOW: f32 = 1.5;
/// Wall seconds a portal has to land before it counts as refused.
pub const PORTAL_WAIT: f64 = 15.0;
/// Wall seconds a recall has to land: its animation plays out before the trip, a lifestone
/// recall's for longer than a portal's whole trip takes.
pub const RECALL_WAIT: f64 = 40.0;
/// Wall seconds to take up a caster and go into the magic stance.
const READY_WAIT: f64 = 12.0;
/// Wall seconds in the magic stance before casting: the stance's own motion plays out first.
const STANCE_SETTLE: f64 = 1.5;
/// A portal is used from this close.
const PORTAL_REACH: f32 = 3.0;
/// An object whose own place a body cannot walk to (a shopkeeper behind a counter, someone in a
/// booth with a window to the street) is walked to within this of it: inside the distance an
/// object is used from.
pub const OBJECT_REACH: f32 = 3.0;
/// The spell that sends a character to where the last portal it used sent it.
pub const PORTAL_RECALL: u32 = 2645;
/// The server's word that a cast fizzled (`Communication_WeenieError` code).
pub const FIZZLED: u32 = 0x0402;
/// How near the person of a hand-back the character stands before it uses them or gives back
/// the token, metres: the server walks it the last of the way.
const HAND_BACK_REACH: f32 = 4.0;
/// Wall seconds the person has to give the token after a use, before they are used again; and
/// the most uses.
const TOKEN_WAIT: f64 = 8.0;
const TOKEN_USES: u32 = 3;
/// Wall seconds the token is held before it is given back: the person is still talking as it
/// arrives, and takes nothing until they have done.
const TOKEN_SETTLE: f64 = 2.5;
/// Wall seconds a token given back has to leave the pack before it is given again; and the most
/// gives.
const GIVE_WAIT: f64 = 5.0;
const GIVES: u32 = 4;
/// Casts of one recall that may fizzle before it is given up.
const MAX_CASTS: u32 = 4;

/// Seconds since 1970 by the wall clock, for when knowledge was confirmed.
fn unix_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A world identity as a file name.
#[must_use]
pub fn file_name(world: &str) -> String {
    world
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The landblock of a cell.
fn block_of(cell: CellId) -> LandblockId {
    LandblockId(u16::try_from(cell.0 >> 16).unwrap_or(0))
}

/// Make the landblocks round `a` and `b` resident, so their buildings and cells are there to plan
/// over; a long way apart, only those round `a`.
pub fn preload(land: &DatLandSource, a: CellId, b: CellId) {
    for block in blocks_round(a, b) {
        land.load_block_cells(block);
    }
}

/// The landblocks round `a` and `b`, a landblock's margin round both; a long way apart, only
/// those round `a`.
#[must_use]
pub fn blocks_round(a: CellId, b: CellId) -> Vec<LandblockId> {
    let xy = |c: CellId| {
        let b = block_of(c).0;
        (i32::from(b >> 8), i32::from(b & 0xFF))
    };
    let ((ax, ay), (mut bx, mut by)) = (xy(a), xy(b));
    if (ax - bx).abs() > 6 || (ay - by).abs() > 6 {
        (bx, by) = (ax, ay);
    }
    let mut out = Vec::new();
    for x in ax.min(bx) - 1..=ax.max(bx) + 1 {
        for y in ay.min(by) - 1..=ay.max(by) + 1 {
            if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
                if x <= 0xFE && y <= 0xFE {
                    out.push(LandblockId((x << 8) | y));
                }
            }
        }
    }
    out
}

/// A cell's middle: an interior cell's frame origin, or a land cell's centre on the ground.
#[must_use]
pub fn middle(land: &DatLandSource, cell: CellId) -> Option<Position> {
    if cell.0 & 0xFFFF >= 0x0100 {
        let g = dereth_physics::LandSource::env_cell(land, cell)?;
        return Some(Position::new(
            cell,
            Frame::new(g.frame.origin, Quat::IDENTITY),
        ));
    }
    let (cx, cy) = dereth_physics::landdefs::cell_index_to_xy(u16::try_from(cell.0 & 0xFFFF).ok()?);
    let o = dereth_physics::landdefs::land_cell_origin(
        i32::try_from(cx).ok()?,
        i32::try_from(cy).ok()?,
    );
    let z = land.ground_height(block_of(cell), o.x, o.y)?;
    Some(Position::new(
        cell,
        Frame::new(Vec3::new(o.x, o.y, z), Quat::IDENTITY),
    ))
}

/// A planner over `land`, its setups and outdoor placements read from `store` as needed.
///
/// # Errors
/// The region's data does not read.
pub fn planner(land: &Arc<DatLandSource>, store: &Arc<RetailDatStore>) -> Result<Planner, String> {
    let memo: Mutex<HashMap<u32, Option<Arc<SetupGeometry>>>> = Mutex::new(HashMap::new());
    let setup_store = Arc::clone(store);
    let setups: Setups = Box::new(move |id| {
        let Ok(mut m) = memo.lock() else { return None };
        m.entry(id.0)
            .or_insert_with(|| {
                dereth_world_data::env_cells::static_geometry(
                    &setup_store,
                    id,
                    &mut dereth_world_data::setup::SetupPartStats::default(),
                )
                .map(Arc::new)
            })
            .clone()
    });
    let region = dereth_world_data::landblock::load_region(store).map_err(|e| e.to_string())?;
    let place_store = Arc::clone(store);
    let placed: Mutex<HashMap<u16, Vec<StaticPlacement>>> = Mutex::new(HashMap::new());
    let placements: Placements = Box::new(move |b: LandblockId| {
        let Ok(mut m) = placed.lock() else {
            return Vec::new();
        };
        m.entry(b.0)
            .or_insert_with(|| {
                crate::world_build::land_placements(&place_store, &region, b.0)
                    .into_iter()
                    .map(|s| StaticPlacement {
                        id: s.id,
                        frame: s.frame,
                        scale: s.scale,
                    })
                    .collect()
            })
            .clone()
    });
    Ok(NavWorld::new(
        LandNavSource {
            land: Arc::clone(land),
            setups,
            land_statics: placements,
        },
        NavConfig::default(),
    ))
}

/// The travel: one journey at a time, and this world's portal and recall knowledge.
pub struct Travel {
    files: StoreFiles,
    portals: PortalStore,
    /// What the server publishes, kept to merge into each world's store as it loads.
    published: Vec<dereth_navigation::PortalEdge>,
    planning: Option<Planning>,
    /// The planning's next job number.
    jobs: u64,
    /// Walking routes asked for (`TravelRequest::Route`): the job, and the request it answers.
    routes_asked: BTreeMap<u64, u64>,
    /// Plans asked for (`TravelRequest::Plan`): the job, and the request it answers.
    plans_asked: BTreeMap<u64, u64>,
    /// The objects last handed to the planning: id, kind and where, to tell when they change.
    obstacles_sent: Vec<(u32, ObstacleKind, u32, [i32; 3])>,
    /// The dungeon whose region was last built ahead of need.
    prepared: Option<u32>,
    character: CharacterState,
    stance: Stance,
    sighted: Vec<PortalSeen>,
    /// The people in sight who send the character on for a token handed back.
    hand_backs: Vec<HandBackSeen>,
    /// The player's answer to the travel's question, this step.
    answer: Option<bool>,
    /// The token of the hand-back under way, when the character holds it.
    token: Option<ObjectId>,
    requests: Vec<TravelRequest>,
    trip: Option<Trip>,
    events: Vec<TravelEvent>,
    /// Plan round creatures standing or ambling near the route.
    pub creatures: bool,
    /// Whether each frame waits for the plans asked for to be answered: for a client stepped
    /// faster than the clock (a test's driver), whose journeys would otherwise outrun their
    /// plans. Off, as a player's client runs, a plan arrives some frames later and the frame
    /// never waits for it.
    pub wait_for_plans: bool,
    forward: bool,
    turning: Option<bool>,
    /// When the character was first seen in the magic stance, while getting ready to cast.
    magic_since: Option<f64>,
    obstacles: Vec<WorldObstacle>,
    /// Every creature seen this frame, for the log.
    creatures_seen: Vec<(String, Vec3)>,
    /// Where each creature was, and when, to tell standing from running.
    creature_track: HashMap<u32, (Vec3, f64, f32)>,
    now: f64,
    wall: f64,
    wall_before: f64,
}

impl std::fmt::Debug for Travel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Travel")
            .field("files", &self.files)
            .field("world", &self.portals.world)
            .field("trip", &self.trip.as_ref().map(|t| t.id))
            .finish_non_exhaustive()
    }
}

impl Travel {
    /// A travel keeping its store in `files`.
    #[must_use]
    pub fn new(files: StoreFiles) -> Self {
        Self {
            files,
            portals: PortalStore::default(),
            published: Vec::new(),
            planning: None,
            jobs: 0,
            routes_asked: BTreeMap::new(),
            plans_asked: BTreeMap::new(),
            obstacles_sent: Vec::new(),
            prepared: None,
            character: CharacterState::default(),
            stance: Stance::default(),
            sighted: Vec::new(),
            hand_backs: Vec::new(),
            answer: None,
            token: None,
            requests: Vec::new(),
            trip: None,
            events: Vec::new(),
            creatures: true,
            wait_for_plans: false,
            forward: false,
            turning: None,
            magic_since: None,
            obstacles: Vec::new(),
            creatures_seen: Vec::new(),
            creature_track: HashMap::new(),
            now: 0.0,
            wall: 0.0,
            wall_before: 0.0,
        }
    }

    /// This world's portal and recall knowledge.
    #[must_use]
    pub const fn store(&self) -> &PortalStore {
        &self.portals
    }

    /// Ask for something; it is taken up at the next step.
    pub fn request(&mut self, r: TravelRequest) {
        self.requests.push(r);
    }

    /// Whether a journey is under way.
    #[must_use]
    pub const fn busy(&self) -> bool {
        self.trip.is_some()
    }

    /// Whether the planner (the world's walkable routes, read from the game's data) has been
    /// built: the first time a journey or a plan is asked for, not before.
    #[must_use]
    pub const fn planner_built(&self) -> bool {
        self.planning.is_some()
    }

    /// Whether the travel has anything to do this frame: a journey under way, a request to take
    /// up, or a plan still to be answered. With nothing, the frame need gather nothing for it.
    #[must_use]
    pub fn wanted(&self) -> bool {
        self.trip.is_some()
            || !self.requests.is_empty()
            || self.planning.as_ref().is_some_and(Planning::waiting)
    }

    /// A frame with nothing else to do, the body at `at`: once the travel has planned at all,
    /// the region of a dungeon the body enters is built, ahead of any need for it.
    pub fn idle(&mut self, at: Option<&Position>) {
        let (Some(p), Some(planning)) = (at, self.planning.as_mut()) else {
            return;
        };
        if p.cell.0 & 0xFFFF < 0x0100 {
            return;
        }
        let dungeon = p.cell.0 >> 16;
        if self.prepared != Some(dungeon) {
            self.prepared = Some(dungeon);
            planning.ask(Job::Prepare(*p));
        }
        // Where the planning shares the frame's thread, this is its slice; nothing is asked, so
        // nothing is answered.
        let _ = planning.answers();
    }

    /// The planning's next job number.
    fn job(&mut self) -> u64 {
        self.jobs += 1;
        self.jobs
    }

    /// Keep `world`'s portal and recall knowledge loaded (read when the world changes), for a
    /// frame with nothing else to do.
    pub fn note_world(&mut self, world: &str) {
        self.load_store(world);
    }

    /// The events since the last take.
    pub fn take_events(&mut self) -> Vec<TravelEvent> {
        std::mem::take(&mut self.events)
    }

    /// The journey in progress, for display.
    #[must_use]
    pub fn view(&self) -> Option<TripView> {
        let t = self.trip.as_ref()?;
        let goal = t.goal?;
        let mut route = Vec::new();
        if let Some(f) = t.follower.as_ref() {
            let next = f.next_waypoint();
            route.extend(f.path().waypoints.iter().skip(next).map(|w| w.position));
        }
        if let Some(Step::Approach { portal }) = &t.step {
            if route.is_empty() {
                route.push(*portal);
            }
        }
        // Where the leg being carried out goes: a walk's end, the portal, a recall's landing.
        let target = t
            .journey
            .as_ref()
            .and_then(|j| j.legs.get(t.leg))
            .map(|l| match l {
                Leg::Walk { to } => *to,
                Leg::Portal { edge } => edge.from,
                Leg::Teleport { teleport } => teleport.to,
                Leg::HandBack { hand_back } => hand_back.at,
            });
        let mut shown = t.leg;
        if let Some(j) = t.journey.as_ref() {
            route.extend(j.legs.iter().skip(t.leg).filter_map(|l| match l {
                Leg::Walk { to } => Some(*to),
                Leg::Portal { edge } => edge.to,
                Leg::Teleport { teleport } => Some(teleport.to),
                Leg::HandBack { hand_back } => Some(hand_back.to),
            }));
            // The shown legs split the first walk at its doors: past it, the index moves on by as
            // many legs as that split added.
            if t.leg > 0 {
                shown += t.view.len().saturating_sub(j.legs.len());
            }
        }
        Some(TripView {
            id: t.id,
            legs: t.view.clone(),
            leg: shown,
            target,
            route,
            goal,
        })
    }

    /// The planning, started over `land` the first time it is needed: false when the world's
    /// data does not read.
    fn planning(&mut self, land: &Arc<DatLandSource>, store: &Arc<RetailDatStore>) -> bool {
        if self.planning.is_none() {
            match Planning::new(land, store) {
                Ok(p) => {
                    self.planning = Some(p);
                    // The objects already known (the doors, the creatures near): the first plan
                    // goes round them as any later one does.
                    self.obstacles_sent.clear();
                    self.send_obstacles();
                }
                Err(e) => {
                    tracing::warn!(target: "travel", "no planner: {e}");
                    return false;
                }
            }
        }
        true
    }

    /// Hand the planning the objects in the world, when they have changed since it was last
    /// handed them.
    fn send_obstacles(&mut self) {
        let Some(planning) = self.planning.as_mut() else {
            return;
        };
        let now: Vec<(u32, ObstacleKind, u32, [i32; 3])> = self
            .obstacles
            .iter()
            .map(|o| {
                let v = o.obstacle.position.frame.origin;
                (
                    o.obstacle.id.0,
                    o.obstacle.kind,
                    o.obstacle.position.cell.0,
                    [to_i32(v.x * 4.0), to_i32(v.y * 4.0), to_i32(v.z * 4.0)],
                )
            })
            .collect();
        if now == self.obstacles_sent {
            return;
        }
        self.obstacles_sent = now;
        planning.ask(Job::Obstacles(
            self.obstacles.iter().map(|o| o.obstacle.clone()).collect(),
        ));
    }

    fn store_path(&self, name: &str) -> Option<PathBuf> {
        Some(
            self.files
                .dir
                .as_ref()?
                .join(format!("{}{name}.tsv", self.files.prefix)),
        )
    }

    /// Read `world`'s store and the hints, when the world changed.
    fn load_store(&mut self, world: &str) {
        if self.portals.world == world {
            return;
        }
        self.portals = PortalStore::new(world);
        if let Some(path) = self.store_path(&file_name(world)) {
            match crate::platform::files::read_to_string(&path) {
                Ok(text) => match PortalStore::from_text(&text) {
                    Ok(s) => {
                        self.portals = s;
                        self.portals.world = world.to_owned();
                    }
                    Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
            }
        }
        if let Some(path) = self.store_path("hints") {
            if let Ok(text) = crate::platform::files::read_to_string(&path) {
                match PortalStore::from_text(&text) {
                    Ok(h) => self.portals.merge(h.portals().iter().cloned()),
                    Err(e) => tracing::warn!(target: "travel", "hints: {e}"),
                }
            }
        }
        self.portals.merge(self.published.iter().cloned());
        let learned = self
            .portals
            .portals()
            .iter()
            .filter(|e| e.source == EdgeSource::Learned)
            .count();
        tracing::info!(
            target: "travel",
            "portal store for {world:?}: {} portal edges ({learned} learned), {} recalls",
            self.portals.portals().len(),
            self.portals.recalls().len()
        );
    }

    /// Take in the portal table the server publishes (the store's text form, its edges of
    /// source `published`): merged now, and into each world's store as it loads.
    ///
    /// # Errors
    /// The text is not a store.
    pub fn set_published(&mut self, text: &str) -> Result<usize, String> {
        let table = PortalStore::from_text(text).map_err(|e| e.to_string())?;
        self.published = table
            .portals()
            .iter()
            .filter(|e| e.source == EdgeSource::Published)
            .cloned()
            .collect();
        self.portals.merge(self.published.iter().cloned());
        tracing::info!(
            target: "travel",
            "the server publishes {} portals",
            self.published.len()
        );
        Ok(self.published.len())
    }

    fn save_store(&self) {
        let Some(path) = self.store_path(&file_name(&self.portals.world)) else {
            return;
        };
        if self.portals.world.is_empty() {
            return;
        }
        // Hints and the server's table stay out: the world's file holds what was learned.
        let text = self
            .portals
            .without(EdgeSource::Hint)
            .without(EdgeSource::Published)
            .to_text();
        if let Err(e) = crate::platform::files::write(&path, text) {
            tracing::warn!(target: "travel", "{}: {e}", path.display());
        }
    }

    /// The server answered a use of `door` as locked (the `OpenFailDueToLock` sound on it): the
    /// trip's walk plans round it.
    pub fn door_answered_locked(&mut self, door: ObjectId) {
        if let Some(t) = self.trip.as_mut() {
            t.lock_answers.insert(door.0);
        }
    }

    /// The objects the world holds now: doors with whether they stand open, other solid objects,
    /// and creatures. Replaces the last list. A creature is kept only while it stands or ambles
    /// within reach of the body at `at`.
    pub fn set_obstacles(&mut self, list: Vec<WorldObstacle>, at: Option<&Position>) {
        let now = self.now;
        let locked = self
            .trip
            .as_ref()
            .map(|t| t.locked.clone())
            .unwrap_or_default();
        let mut kept = Vec::with_capacity(list.len());
        self.creatures_seen.clear();
        let mut seen = BTreeMap::new();
        for mut o in list {
            if locked.contains(&o.obstacle.id.0) {
                o.obstacle.kind = ObstacleKind::Door(DoorState::Locked);
            }
            if o.obstacle.kind == ObstacleKind::Creature {
                let g = global(&o.obstacle.position);
                self.creatures_seen.push((o.name.clone(), g));
                // Speed over the last half second or more of game time; unknown (fast) until the
                // creature has been seen that long.
                let (speed, entry) = match self.creature_track.get(&o.obstacle.id.0) {
                    Some(&(p, t, _)) if now - t >= 0.5 => {
                        #[allow(clippy::cast_possible_truncation)]
                        let dt = (now - t) as f32;
                        let v = hypotf(g.x - p.x, g.y - p.y) / dt;
                        (v, (g, now, v))
                    }
                    Some(&(p, t, v)) => (v, (p, t, v)),
                    None => (f32::MAX, (g, now, f32::MAX)),
                };
                seen.insert(o.obstacle.id.0, entry);
                let near = at.is_some_and(|a| {
                    let h = global(a);
                    hypotf(h.x - g.x, h.y - g.y) < CREATURE_REACH
                });
                if !self.creatures || !near || speed > CREATURE_SLOW {
                    continue;
                }
            }
            kept.push(o);
        }
        self.creature_track = seen.into_iter().collect();
        // A new obstacle in the way of the route ahead, or a door that has closed on it, sends the
        // body round by a new route before it walks into it.
        if let Some(t) = self.trip.as_mut().filter(|t| t.route_dirty.is_none()) {
            if let Some(f) = t.follower.as_ref() {
                let next = f.next_waypoint().saturating_sub(1);
                let route: Vec<Vec3> = f
                    .path()
                    .legs
                    .iter()
                    .skip(next)
                    .flatten()
                    .map(global)
                    .collect();
                for o in &kept {
                    let blocks = o.obstacle.kind != ObstacleKind::Door(DoorState::Open);
                    let known = self.obstacles.iter().any(|k| {
                        k.obstacle.id == o.obstacle.id && k.obstacle.kind == o.obstacle.kind
                    });
                    if !blocks || known {
                        continue;
                    }
                    let q = global(&o.obstacle.position);
                    let reach = if o.obstacle.kind == ObstacleKind::Creature {
                        2.0
                    } else {
                        3.0
                    };
                    if route
                        .iter()
                        .any(|r| hypotf(r.x - q.x, r.y - q.y) < reach && (r.z - q.z).abs() < 3.0)
                    {
                        t.route_dirty = Some(format!("{} {:08X}", o.name, o.obstacle.id.0));
                        break;
                    }
                }
            }
        }
        self.obstacles = kept;
        self.send_obstacles();
    }

    /// The objects and creatures within `reach` across of `p`, nearest first, for the log.
    fn near(&self, p: &Position, reach: f32) -> Vec<String> {
        let here = global(p);
        let creatures = self.creatures_seen.iter().filter_map(|(name, q)| {
            let d = hypotf(q.x - here.x, q.y - here.y);
            (d <= reach).then(|| (d, format!("creature {name} at {d:.1} m")))
        });
        let mut v: Vec<(f32, String)> = self
            .obstacles
            .iter()
            .filter(|o| o.obstacle.kind != ObstacleKind::Creature)
            .filter_map(|o| {
                let q = global(&o.obstacle.position);
                let d = hypotf(q.x - here.x, q.y - here.y);
                (d <= reach).then(|| {
                    (
                        d,
                        format!(
                            "{} {:08X} {:?} at {d:.1} m",
                            o.name, o.obstacle.id.0, o.obstacle.kind
                        ),
                    )
                })
            })
            .chain(creatures)
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|(_, s)| s).collect()
    }

    fn release_keys(&mut self, out: &mut Vec<TravelCommand>) {
        if self.forward {
            out.push(TravelCommand::Forward(false));
            self.forward = false;
        }
        if self.turning.is_some() {
            out.push(TravelCommand::Turn(None));
            self.turning = None;
        }
    }

    /// End the trip: arrived when `arrived`, otherwise failed with `reason`.
    fn end(&mut self, out: &mut Vec<TravelCommand>, at: Option<&Position>, failed: Option<&str>) {
        self.release_keys(out);
        let Some(mut t) = self.trip.take() else {
            return;
        };
        t.stats.game_seconds = self.now - t.started;
        t.stats.wall_seconds = self.wall - t.started_wall;
        if t.leave_stance && self.stance.magic {
            out.push(TravelCommand::ToggleCombat);
        }
        match failed {
            None => {
                let distance = match (at, t.goal) {
                    (Some(p), Some(g)) => {
                        let (a, b) = (global(p), global(&g));
                        hypotf(a.x - b.x, a.y - b.y)
                    }
                    _ => 0.0,
                };
                tracing::info!(
                    target: "travel",
                    "journey {} arrived: {distance:.2} m from the goal; {:?}",
                    t.id,
                    t.stats
                );
                self.events.push(TravelEvent::Arrived {
                    id: t.id,
                    distance,
                    stats: t.stats,
                });
            }
            Some(reason) => {
                tracing::warn!(target: "travel", "journey {} failed: {reason}; {:?}", t.id, t.stats);
                self.events.push(TravelEvent::Failed {
                    id: t.id,
                    reason: reason.to_owned(),
                    stats: t.stats,
                });
            }
        }
    }

    /// Where a target is, as the frame knows it: the planning takes it to a place to stand.
    fn goal_of(target: TravelTarget, input: &TravelInput<'_>) -> Option<Goal> {
        match target {
            TravelTarget::Place(p) => Some(Goal::Place(p)),
            TravelTarget::Object(o) => (input.object)(o).map(Goal::Place),
            TravelTarget::Cell(c) => Some(Goal::Cell(c)),
        }
    }

    /// The teleports the character can use now under `allow`, less those that failed on `trip`.
    fn teleports(&self, allow: TravelAllow, disabled: &BTreeSet<Recall>) -> Vec<Teleport> {
        let mut c = self.character.clone();
        if !allow.spells {
            c.recall_spells.clear();
        }
        if !allow.items {
            c.recall_items.clear();
        }
        if !allow.commands {
            c.recall_commands.clear();
        }
        c.teleports(&self.portals)
            .into_iter()
            .filter(|t| !disabled.contains(&t.recall))
            .collect()
    }

    /// Ask the planning for the journey from `from` to `to` under `allow`, less the recalls in
    /// `disabled`; its job, or `None` with no planning. `object` says the journey ends at an
    /// object, so a journey that is one walk ends within reach of it.
    fn ask_journey(
        &mut self,
        from: &Position,
        to: Goal,
        allow: TravelAllow,
        disabled: &BTreeSet<Recall>,
        object: bool,
    ) -> Option<u64> {
        let teleports = self.teleports(allow, disabled);
        let (portals, hand_backs) = if allow.portals {
            (
                self.portals
                    .routable(self.character.level)
                    .into_iter()
                    .cloned()
                    .collect(),
                self.hand_backs(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        self.planning.as_ref()?;
        let id = self.job();
        self.planning.as_mut()?.ask(Job::Journey {
            id,
            from: *from,
            to,
            portals,
            teleports,
            hand_backs,
            reach: object.then_some(OBJECT_REACH),
        });
        Some(id)
    }

    /// The hand-backs the people in sight offer: each sends the character where a portal of their
    /// own dungeon that leads out of it sends (the exit their dungeon has besides them), as the
    /// store knows it from the server or from a use. A person whose dungeon has no such portal
    /// known offers none.
    fn hand_backs(&self) -> Vec<HandBack> {
        let block = |p: &Position| p.cell.0 >> 16;
        self.hand_backs
            .iter()
            .filter_map(|h| {
                let to = self
                    .portals
                    .portals()
                    .iter()
                    .filter_map(|e| Some((e.from, e.to?)))
                    .find(|(from, to)| block(from) == block(&h.at) && block(to) != block(&h.at))
                    .map(|(_, to)| to)?;
                Some(HandBack {
                    name: h.name.clone(),
                    object: Some(h.object),
                    at: h.at,
                    token: h.token.clone(),
                    to,
                })
            })
            .collect()
    }

    /// The legs as shown: the first walk's route (`first`, when planned) in detail, its doors
    /// legs of their own.
    fn legs_of(j: &Journey, first: Option<&NavPath>) -> Vec<TravelLeg> {
        let mut out = Vec::new();
        for (i, l) in j.legs.iter().enumerate() {
            match l {
                Leg::Walk { to } if i == 0 => out.extend(walk_legs(*to, first)),
                other => out.push(TravelLeg::of(other)),
            }
        }
        out
    }

    /// Ask for the trip's journey from `from`; its first leg starts once it is planned.
    fn plan_trip(&mut self, from: &Position, out: &mut Vec<TravelCommand>, reason: Option<String>) {
        let Some(t) = self.trip.as_ref() else { return };
        let (goal, allow, disabled) = match t.goal {
            Some(g) => (Goal::Place(g), t.allow, t.disabled.clone()),
            None => return,
        };
        let object = matches!(t.target, TravelTarget::Object(_));
        self.ask_trip(from, goal, allow, &disabled, object, reason, out);
    }

    /// Ask for the trip's journey from `from` to `goal`, and stand while it is planned.
    #[allow(clippy::too_many_arguments)]
    fn ask_trip(
        &mut self,
        from: &Position,
        goal: Goal,
        allow: TravelAllow,
        disabled: &BTreeSet<Recall>,
        object: bool,
        reason: Option<String>,
        out: &mut Vec<TravelCommand>,
    ) {
        let c = &self.character;
        tracing::info!(
            target: "travel",
            "recalls for {:?}: spells {:?}, items {:?}, by command {}; can cast {}, mana {:?}; \
             allowed {allow:?}; stance {:?}",
            c.name,
            c.recall_spells,
            c.recall_items,
            c.recall_commands.len(),
            c.can_cast,
            c.mana,
            self.stance
        );
        let Some(job) = self.ask_journey(from, goal, allow, disabled, object) else {
            self.end(out, Some(from), Some("no_path"));
            return;
        };
        self.release_keys(out);
        if let Some(t) = self.trip.as_mut() {
            t.follower = None;
            t.walking = None;
            t.ahead = None;
            t.step = Some(Step::Planning { job, reason });
        }
    }

    /// The trip's journey is planned: tell it and start its first leg from `here`.
    fn planned(
        &mut self,
        here: &Position,
        to: Option<Position>,
        planned: Option<(Journey, f64)>,
        first: Option<&NavPath>,
        reason: Option<String>,
        out: &mut Vec<TravelCommand>,
    ) {
        let Some(t) = self.trip.as_mut() else { return };
        t.step = None;
        let id = t.id;
        let Some(to) = to else {
            self.end(out, Some(here), Some("no_target"));
            return;
        };
        if t.goal.is_none() {
            t.goal = Some(to);
        }
        let Some((journey, ms)) = planned else {
            self.end(out, Some(here), Some("no_path"));
            return;
        };
        let view = Self::legs_of(&journey, first);
        tracing::info!(
            target: "travel",
            "journey {id}: {:.0} s expected, planned in {ms:.0} ms: {}",
            journey.seconds,
            describe(&journey)
        );
        self.events.push(TravelEvent::Planned {
            id,
            plan: TravelPlan {
                legs: view.clone(),
                seconds: journey.seconds,
            },
            reason,
        });
        let hand_back = journey.legs.iter().find_map(|l| match l {
            Leg::HandBack { hand_back } => Some(hand_back.clone()),
            _ => None,
        });
        let mut ask = None;
        if let Some(t) = self.trip.as_mut() {
            t.stats.journeys += 1;
            t.journey = Some(journey);
            t.view = view;
            t.leg = 0;
            // Handing a token back is for good: the player says yes first, once a trip.
            if let Some(h) = hand_back.filter(|_| !t.confirmed) {
                t.step = Some(Step::Ask);
                ask = Some(h);
            }
        }
        if let Some(h) = ask {
            self.release_keys(out);
            let question = self
                .hand_backs
                .iter()
                .find(|s| Some(s.object) == h.object || s.name == h.name)
                .map_or_else(
                    || {
                        format!(
                            "This journey hands the {} back to {}. Go on?",
                            h.token, h.name
                        )
                    },
                    |s| s.question.clone(),
                );
            tracing::info!(target: "travel", "asking the player: {question}");
            out.push(TravelCommand::Ask(question));
            return;
        }
        self.start_leg(here, out);
    }

    /// Start the trip's leg `leg` from `here`.
    fn start_leg(&mut self, here: &Position, out: &mut Vec<TravelCommand>) {
        let Some(t) = self.trip.as_ref() else { return };
        let (id, index) = (t.id, t.leg);
        let Some(leg) = t.journey.as_ref().and_then(|j| j.legs.get(index)).cloned() else {
            self.end(out, Some(here), None);
            return;
        };
        self.events.push(TravelEvent::Leg {
            id,
            index,
            leg: TravelLeg::of(&leg),
        });
        match leg {
            // The walk to a hand-back's person is the hand-back's own: it ends within reach of
            // them, not where they stand.
            Leg::Walk { to }
                if self.trip.as_ref().is_some_and(|t| {
                    matches!(
                        t.journey.as_ref().and_then(|j| j.legs.get(index + 1)),
                        Some(Leg::HandBack { hand_back }) if hand_back.at == to
                    )
                }) =>
            {
                if let Some(t) = self.trip.as_mut() {
                    t.leg += 1;
                }
                self.start_leg(here, out);
            }
            Leg::Walk { to } => {
                self.set_step(Step::Walk {
                    target: to,
                    last: true,
                });
                self.walk_toward(here, &to, out);
            }
            Leg::Portal { edge } => {
                self.set_step(Step::Approach { portal: edge.from });
                self.walk_toward(here, &edge.from, out);
            }
            Leg::Teleport { teleport } => {
                self.release_keys(out);
                self.start_recall(here, teleport.recall, teleport.item, out);
            }
            Leg::HandBack { hand_back } => {
                let at = hand_back.at;
                self.set_step(Step::HandBack {
                    hand_back,
                    tries: HandBackTries::default(),
                });
                if global_distance(here, &at) <= HAND_BACK_REACH {
                    self.hand_back(here, out);
                } else {
                    self.walk_toward(here, &at, out);
                }
            }
        }
    }

    /// At the person of a hand-back: take the token from them (using them, again a while later
    /// when it does not come), give it back once they have done talking (again a while later
    /// while it stays in the pack), and once it has left the pack wait to be sent on.
    fn hand_back(&mut self, here: &Position, out: &mut Vec<TravelCommand>) {
        let Some(Step::HandBack {
            hand_back,
            mut tries,
        }) = self.trip.as_ref().and_then(|t| t.step.clone())
        else {
            return;
        };
        self.release_keys(out);
        if let Some(t) = self.trip.as_mut() {
            t.follower = None;
        }
        let person = hand_back.object.or_else(|| {
            self.hand_backs
                .iter()
                .find(|s| s.name == hand_back.name)
                .map(|s| s.object)
        });
        let Some(person) = person else {
            self.end(out, Some(here), Some("no_person"));
            return;
        };
        let (name, token) = (hand_back.name.clone(), hand_back.token.clone());
        match tries.next(self.token, self.wall) {
            HandBackMove::Wait => {}
            HandBackMove::Use(n) => {
                tracing::info!(target: "travel", "using {name} for the {token} ({n})");
                out.push(TravelCommand::Use(person));
            }
            HandBackMove::Give(item, n) => {
                tracing::info!(
                    target: "travel",
                    "handing the {token} {:08X} back to {name} ({n})",
                    item.0
                );
                out.push(TravelCommand::Give {
                    item,
                    target: person,
                });
            }
            HandBackMove::Taken => {
                tracing::info!(target: "travel", "{name} took the {token} back");
                let wait = self.wait(Awaited::HandBack(name), here);
                self.set_step(wait);
                return;
            }
            HandBackMove::NoToken(n) => {
                tracing::warn!(target: "travel", "{name} gave no {token} in {n} uses");
                self.end(out, Some(here), Some("no_token"));
                return;
            }
            HandBackMove::NotTaken(n) => {
                tracing::warn!(target: "travel", "{name} did not take the {token} back in {n} gives");
                self.end(out, Some(here), Some("not_taken"));
                return;
            }
        }
        self.set_step(Step::HandBack { hand_back, tries });
    }

    fn set_step(&mut self, step: Step) {
        if let Some(t) = self.trip.as_mut() {
            t.step = Some(step);
        }
    }

    /// Recall by `recall`: an item is used, a command typed, a spell cast once the character is in
    /// the magic stance.
    fn start_recall(
        &mut self,
        here: &Position,
        recall: Recall,
        item: Option<ObjectId>,
        out: &mut Vec<TravelCommand>,
    ) {
        tracing::info!(target: "travel", "recalling by {}", recall.word());
        match (recall, item) {
            (_, Some(item)) => out.push(TravelCommand::Use(item)),
            (Recall::Spell(_), None) => {
                self.set_step(Step::Ready {
                    recall,
                    since: self.wall,
                    wielded: false,
                    toggled: false,
                });
                self.ready(here, out);
                return;
            }
            (Recall::Lifestone, None) => out.push(TravelCommand::Chat(String::from("/lifestone"))),
            (Recall::Marketplace, None) => {
                out.push(TravelCommand::Chat(String::from("/marketplace")));
            }
        }
        let wait = self.wait(Awaited::Recall(recall), here);
        self.set_step(wait);
    }

    /// Get ready to cast: take up a caster, go into the magic stance, then cast.
    fn ready(&mut self, here: &Position, out: &mut Vec<TravelCommand>) {
        let Some(Step::Ready {
            recall,
            since,
            wielded,
            toggled,
        }) = self.trip.as_ref().and_then(|t| t.step.clone())
        else {
            return;
        };
        let Recall::Spell(spell) = recall else { return };
        // In the stance, settled into it, and not busy: as a player waits for the stance to come
        // up before casting.
        let settled = match (self.stance.magic, self.magic_since) {
            (true, Some(t)) => self.wall - t >= STANCE_SETTLE,
            (true, None) => {
                self.magic_since = Some(self.wall);
                false
            }
            (false, _) => {
                self.magic_since = None;
                false
            }
        };
        if settled && !self.stance.busy {
            tracing::info!(target: "travel", "casting {spell}");
            out.push(TravelCommand::Cast(spell));
            if let Some(t) = self.trip.as_mut() {
                t.leave_stance |= toggled;
                t.casts += 1;
            }
            let wait = self.wait(Awaited::Recall(recall), here);
            self.set_step(wait);
            return;
        }
        if self.wall - since > READY_WAIT {
            self.recall_failed(here, recall, "could not go into the magic stance", out);
            return;
        }
        let (mut wielded, mut toggled) = (wielded, toggled);
        if !self.stance.caster_wielded {
            if let (false, Some(c)) = (wielded, self.stance.caster_carried) {
                tracing::info!(target: "travel", "taking up caster {:08X}", c.0);
                out.push(TravelCommand::Wield(c));
                wielded = true;
            }
        } else if !toggled {
            tracing::info!(target: "travel", "going into the magic stance");
            out.push(TravelCommand::ToggleCombat);
            toggled = true;
        }
        self.set_step(Step::Ready {
            recall,
            since,
            wielded,
            toggled,
        });
    }

    /// Ask for the route as far toward `to` as one window holds; the body stands until it comes,
    /// and then follows it.
    fn walk_toward(&mut self, here: &Position, to: &Position, out: &mut Vec<TravelCommand>) {
        // The walk to the object a journey goes to ends within reach of it.
        let reach = self
            .trip
            .as_ref()
            .filter(|t| matches!(t.target, TravelTarget::Object(_)) && t.goal == Some(*to))
            .map(|_| OBJECT_REACH);
        let id = self.job();
        let Some(planning) = self.planning.as_mut() else {
            self.end(out, Some(here), Some("no_route"));
            return;
        };
        // A portal's own spot may be no place to stand: aim for the nearest one.
        planning.ask(Job::Route {
            id,
            from: *here,
            to: Goal::Place(*to),
            reach,
            snap: reach.is_none().then_some(6.0),
        });
        self.release_keys(out);
        if let Some(t) = self.trip.as_mut() {
            t.follower = None;
            t.walking = Some((id, *to));
            t.ahead = None;
        }
    }

    /// The route the walk waited for: follow it, or with none, try again or give up.
    fn walked(
        &mut self,
        here: &Position,
        to: Position,
        route: Result<(NavPath, bool), NoPath>,
        ms: f64,
        out: &mut Vec<TravelCommand>,
    ) {
        match route {
            Ok((path, last)) => {
                tracing::info!(
                    target: "travel",
                    "route: {:.1} m, {} waypoints, {} cells, from {:08X} {:?}{}, planned in \
                     {ms:.0} ms",
                    path.length(),
                    path.waypoints.len(),
                    path.cells().len(),
                    here.cell.0,
                    here.frame.origin,
                    if last { "" } else { " (to the window's edge)" }
                );
                self.follow_route(path, last);
            }
            Err(e) => {
                tracing::warn!(target: "travel", "no route from {:?}: {e}", here.frame.origin);
                let Some(t) = self.trip.as_mut() else { return };
                t.follower = None;
                t.stats.replans += 1;
                if t.stats.replans > MAX_REPLANS {
                    self.end(out, Some(here), Some("no_route"));
                    return;
                }
                // The places taken out where the body stopped may have been the only way: put
                // them back and plan once more.
                if !t.unblocked {
                    t.unblocked = true;
                    if let Some(planning) = self.planning.as_mut() {
                        planning.ask(Job::Unblock);
                    }
                    self.walk_toward(here, &to, out);
                    return;
                }
                if t.locked.iter().all(|d| t.lock_answers.contains(d)) {
                    // Where only a drop leads on, the journey names it: a jump there is the
                    // player's to make.
                    let why = match e {
                        NoPath::OnlyByDrop { .. } => format!("only_by_drop: {e}"),
                        _ => String::from("no_route"),
                    };
                    self.end(out, Some(here), Some(&why));
                    return;
                }
                // A door that did not open may only have been slow, or had something standing in
                // it: with no way round, try those doors once more. One the server said is locked
                // stays planned round.
                let answered = t.lock_answers.clone();
                tracing::info!(
                    target: "travel",
                    "trying the {} doors that did not open again",
                    t.locked.len() - answered.len()
                );
                t.locked.retain(|d| answered.contains(d));
                let mut list = std::mem::take(&mut self.obstacles);
                for o in &mut list {
                    if o.obstacle.kind == ObstacleKind::Door(DoorState::Locked)
                        && !answered.contains(&o.obstacle.id.0)
                    {
                        o.obstacle.kind = ObstacleKind::Door(DoorState::Closed);
                    }
                }
                self.set_obstacles(list, Some(here));
                if let Some(t) = self.trip.as_mut() {
                    t.route_dirty = None;
                }
                self.walk_toward(here, &to, out);
            }
        }
    }

    /// Follow `path`, the walk's route: to its target when `last`, else to a window's edge.
    fn follow_route(&mut self, path: NavPath, last: bool) {
        let Some(t) = self.trip.as_mut() else { return };
        t.unblocked = false;
        if let Some(Step::Walk { last: l, .. }) = t.step.as_mut() {
            *l = last;
        }
        // The leg being walked, shown with its route and doors.
        if let (Some(j), true) = (t.journey.as_ref(), t.leg == 0) {
            if let Some(Leg::Walk { to }) = j.legs.first() {
                let rest: Vec<TravelLeg> = t
                    .view
                    .iter()
                    .skip_while(|l| matches!(l, TravelLeg::Walk { .. } | TravelLeg::Door { .. }))
                    .cloned()
                    .collect();
                t.view = walk_legs(*to, Some(&path));
                t.view.extend(rest);
            }
        }
        t.follower = Some(Follower::new(path, FollowConfig::default()));
    }

    /// The portal in sight nearest `at`, within `reach`.
    fn portal_near(&self, at: &Position, reach: f32) -> Option<&PortalSeen> {
        self.sighted
            .iter()
            .map(|s| (global_distance(&s.sighting.position, at), s))
            .filter(|(d, _)| *d < reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, s)| s)
    }

    /// The first recall the character has not learned the destination of.
    fn untried_recall(&self, disabled: &BTreeSet<Recall>) -> Option<(Recall, Option<ObjectId>)> {
        let c = &self.character;
        let untried = |r: Recall| {
            self.portals.recall_destination(&c.name, r).is_none() && !disabled.contains(&r)
        };
        let items = c
            .recall_items
            .iter()
            .map(|(item, s, _)| (Recall::Spell(*s), Some(*item)));
        let spells = c
            .recall_spells
            .iter()
            .filter(|_| c.can_cast)
            .map(|(s, _, _)| (Recall::Spell(*s), None));
        let commands = c.recall_commands.iter().map(|(r, _)| (*r, None));
        items
            .chain(spells)
            .chain(commands)
            .find(|(r, _)| untried(*r))
    }

    /// The step that waits for what `what` started to land, from `before`.
    fn wait(&self, what: Awaited, before: &Position) -> Step {
        let wait = match what {
            Awaited::Portal(_) | Awaited::WalkedInto(_) | Awaited::Elsewhere => PORTAL_WAIT,
            Awaited::Recall(_) | Awaited::HandBack(_) => RECALL_WAIT,
        };
        Step::Wait {
            what,
            watch: TeleportWatch {
                before: *before,
                since: self.wall,
                wait,
            },
        }
    }

    /// Learning recalls: use the next one not yet learned, or end.
    fn next_recall(&mut self, p: &Position, out: &mut Vec<TravelCommand>) {
        let disabled = self
            .trip
            .as_ref()
            .map(|t| t.disabled.clone())
            .unwrap_or_default();
        match self.untried_recall(&disabled) {
            Some((recall, item)) => {
                tracing::info!(target: "travel", "trying recall {} to learn it", recall.word());
                self.start_recall(p, recall, item, out);
            }
            None => self.end(out, Some(p), None),
        }
    }

    /// After a failed or finished teleport: carry on with the trip's purpose.
    fn carry_on(&mut self, p: &Position, out: &mut Vec<TravelCommand>, reason: Option<String>) {
        match self.trip.as_ref().map(|t| t.purpose) {
            Some(Purpose::LearnRecalls) => self.next_recall(p, out),
            Some(Purpose::Go) => self.plan_trip(p, out, reason),
            None => {}
        }
    }

    fn recall_failed(
        &mut self,
        p: &Position,
        recall: Recall,
        reason: &str,
        out: &mut Vec<TravelCommand>,
    ) {
        tracing::warn!(
            target: "travel",
            "recall {} did not land ({reason}); not offered again",
            recall.word()
        );
        let Some(t) = self.trip.as_mut() else { return };
        t.disabled.insert(recall);
        t.stats.refusals += 1;
        t.stats.replans += 1;
        t.step = None;
        if t.stats.replans > MAX_REPLANS {
            self.end(out, Some(p), Some("too_many_replans"));
            return;
        }
        self.carry_on(p, out, Some(format!("recall {} {reason}", recall.word())));
    }

    /// A walking route asked for is answered.
    fn route_answered(
        &mut self,
        request: u64,
        to: Option<Position>,
        route: Result<(NavPath, bool), NoPath>,
    ) {
        let route = match (to, route) {
            (None, _) => Err(String::from("no_target")),
            (_, Err(NoPath::TooFar)) => Err(String::from("too_far")),
            (_, Err(e @ NoPath::OnlyByDrop { .. })) => Err(format!("only_by_drop: {e}")),
            (_, Err(_)) => Err(String::from("no_route")),
            (Some(_), Ok((path, complete))) => {
                let doors = path
                    .waypoints
                    .iter()
                    .enumerate()
                    .filter_map(|(i, w)| {
                        w.action.map(|WaypointAction::OpenDoor(object)| (object, i))
                    })
                    .collect();
                Ok(TravelRoute {
                    length: path.length(),
                    points: path.waypoints.iter().map(|w| w.position).collect(),
                    doors,
                    complete,
                })
            }
        };
        self.events.push(TravelEvent::Route { id: request, route });
    }

    /// The planning's answers this frame, taken up with the body at `here`.
    fn answers(&mut self, here: Option<&Position>, out: &mut Vec<TravelCommand>) {
        let Some(planning) = self.planning.as_mut() else {
            return;
        };
        let answers = if self.wait_for_plans {
            planning.answers_waited()
        } else {
            planning.answers()
        };
        for a in answers {
            match a {
                Answer::Route { id, to, route, ms } => {
                    if let Some(request) = self.routes_asked.remove(&id) {
                        self.route_answered(request, to, route);
                        continue;
                    }
                    let Some(t) = self.trip.as_mut() else {
                        continue;
                    };
                    if let Some((ahead, None)) = t.ahead.as_mut() {
                        if *ahead == id {
                            t.ahead = Some((id, Some(route)));
                            continue;
                        }
                    }
                    let Some((walk, toward)) = t.walking else {
                        continue;
                    };
                    if walk != id {
                        continue;
                    }
                    t.walking = None;
                    if let Some(p) = here {
                        self.walked(p, toward, route, ms, out);
                    }
                }
                Answer::Journey {
                    id,
                    to,
                    journey,
                    first,
                } => {
                    let first = first.and_then(Result::ok).map(|(p, _)| p);
                    if let Some(request) = self.plans_asked.remove(&id) {
                        let plan = match (to, journey) {
                            (None, _) => Err(String::from("no_target")),
                            (_, None) => Err(String::from("no_path")),
                            (Some(_), Some((j, _))) => Ok(TravelPlan {
                                legs: Self::legs_of(&j, first.as_ref()),
                                seconds: j.seconds,
                            }),
                        };
                        self.events.push(TravelEvent::Answer { id: request, plan });
                        continue;
                    }
                    let Some(reason) = self.trip.as_ref().and_then(|t| match &t.step {
                        Some(Step::Planning { job, reason }) if *job == id => Some(reason.clone()),
                        _ => None,
                    }) else {
                        continue;
                    };
                    if let Some(p) = here {
                        self.planned(p, to, journey, first.as_ref(), reason, out);
                    }
                }
                Answer::Unblocked(n) => {
                    if n > 0 {
                        tracing::info!(target: "travel", "put back {n} places taken out");
                    }
                }
            }
        }
    }

    /// Take up the requests waiting.
    fn take_requests(&mut self, input: &TravelInput<'_>, out: &mut Vec<TravelCommand>) {
        for r in std::mem::take(&mut self.requests) {
            match r {
                TravelRequest::Cancel { id, reason } => {
                    if self.trip.as_ref().is_some_and(|t| t.id == id) {
                        self.end(out, input.at.as_ref(), Some(&reason));
                    }
                }
                TravelRequest::Plan { id, target, allow } => {
                    let asked = (|| {
                        let from = input.at.ok_or("no_position")?;
                        let goal = Self::goal_of(target, input).ok_or("no_target")?;
                        let object = matches!(target, TravelTarget::Object(_));
                        self.ask_journey(&from, goal, allow, &BTreeSet::new(), object)
                            .ok_or("no_world")
                    })();
                    match asked {
                        Ok(job) => {
                            self.plans_asked.insert(job, id);
                        }
                        Err(e) => self.events.push(TravelEvent::Answer {
                            id,
                            plan: Err(e.to_owned()),
                        }),
                    }
                }
                TravelRequest::Go { id, target, allow } => {
                    if self.trip.is_some() {
                        self.end(out, input.at.as_ref(), Some("replaced"));
                    }
                    self.trip = Some(Trip::new(
                        id,
                        Purpose::Go,
                        target,
                        allow,
                        self.now,
                        self.wall,
                    ));
                }
                TravelRequest::Route { id, target } => {
                    let asked = (|| {
                        let from = input.at.ok_or("no_position")?;
                        let goal = Self::goal_of(target, input).ok_or("no_target")?;
                        input.land.ok_or("no_world")?;
                        let reach =
                            matches!(target, TravelTarget::Object(_)).then_some(OBJECT_REACH);
                        let job = self.job();
                        self.planning.as_mut().ok_or("no_world")?.ask(Job::Route {
                            id: job,
                            from,
                            to: goal,
                            reach,
                            snap: Some(12.0),
                        });
                        Ok::<_, &str>(job)
                    })();
                    match asked {
                        Ok(job) => {
                            self.routes_asked.insert(job, id);
                        }
                        Err(e) => self.events.push(TravelEvent::Route {
                            id,
                            route: Err(e.to_owned()),
                        }),
                    }
                }
                TravelRequest::LearnRecalls { id } => {
                    if self.trip.is_some() {
                        self.end(out, input.at.as_ref(), Some("replaced"));
                    }
                    let mut t = Trip::new(
                        id,
                        Purpose::LearnRecalls,
                        TravelTarget::Cell(CellId(0)),
                        TravelAllow::default(),
                        self.now,
                        self.wall,
                    );
                    t.goal = input.at;
                    self.trip = Some(t);
                }
            }
        }
    }

    /// One frame: take up requests, then carry the journey on. Answers what the frame must do.
    pub fn use_time(&mut self, input: &TravelInput<'_>) -> Vec<TravelCommand> {
        self.wall_before = self.wall;
        self.wall = input.wall;
        self.now = input.now;
        self.character = input.character.clone();
        self.stance = input.stance;
        self.sighted.clone_from(&input.portals);
        self.hand_backs.clone_from(&input.hand_backs);
        self.answer = input.answer;
        self.token = match self.trip.as_ref().and_then(|t| t.step.as_ref()) {
            Some(Step::HandBack { hand_back, .. }) => (input.holds)(&hand_back.token),
            _ => None,
        };
        if let Some(w) = input.world.as_deref() {
            self.load_store(w);
        }
        let mut out = Vec::new();
        if let Some(land) = input.land {
            if !self.planning(land, input.store) {
                return out;
            }
        }
        self.take_requests(input, &mut out);
        self.answers(input.at.as_ref(), &mut out);
        if input.player_input && self.trip.is_some() {
            // The player has the controls.
            self.end(&mut out, input.at.as_ref(), Some("interrupted"));
            return out;
        }
        let Some(p) = input.at else {
            return out;
        };
        let Some(t) = self.trip.as_mut() else {
            return out;
        };
        let mut jumped = None;
        if let Some(l) = t.last {
            let (a, b) = (global(&p), global(&l));
            let d = hypotf(a.x - b.x, a.y - b.y);
            // A teleport is no walk.
            if d < 30.0 {
                t.stats.walked += d;
            } else {
                jumped = Some(l);
            }
        }
        t.last = Some(p);
        // Taken away while walking: walking into the portal being approached uses it; anything
        // else is waited out and the journey planned again from where the character lands.
        let mut taken = None;
        if let (Some(before), Some(Step::Walk { .. } | Step::Approach { .. })) =
            (jumped, t.step.as_ref())
        {
            // Walking into the portal being approached uses it, and so does walking into any
            // portal the store knows on the way; anything else is waited out.
            let walked_into = |p: &dereth_navigation::PortalEdge| {
                global_distance(&p.from, &before) < PORTAL_REACH + 3.0
            };
            let known = self
                .portals
                .portals()
                .iter()
                .filter(|e| walked_into(e))
                .min_by(|a, b| {
                    global_distance(&a.from, &before).total_cmp(&global_distance(&b.from, &before))
                })
                .map(|e| PortalSighting {
                    wcid: e.wcid,
                    name: e.name.clone(),
                    position: e.from,
                });
            taken = Some((
                match (&t.step, t.journey.as_ref().and_then(|j| j.legs.get(t.leg))) {
                    (Some(Step::Approach { .. }), Some(Leg::Portal { edge })) => {
                        Awaited::Portal(PortalSighting {
                            wcid: edge.wcid,
                            name: edge.name.clone(),
                            position: edge.from,
                        })
                    }
                    _ => known.map_or(Awaited::Elsewhere, Awaited::WalkedInto),
                },
                before,
            ));
            t.follower = None;
        }
        let (begun, purpose, target) =
            (t.journey.is_none() && t.step.is_none(), t.purpose, t.target);
        if let Some((what, before)) = taken {
            self.release_keys(&mut out);
            let wait = self.wait(what, &before);
            self.set_step(wait);
        }
        if begun {
            // Just begun: find the goal and plan, or start learning recalls.
            match purpose {
                Purpose::LearnRecalls => self.next_recall(&p, &mut out),
                Purpose::Go => match Self::goal_of(target, input) {
                    Some(goal) => {
                        let (allow, object) = self
                            .trip
                            .as_ref()
                            .map_or((TravelAllow::default(), false), |t| {
                                (t.allow, matches!(t.target, TravelTarget::Object(_)))
                            });
                        self.ask_trip(&p, goal, allow, &BTreeSet::new(), object, None, &mut out);
                    }
                    None => self.end(&mut out, Some(&p), Some("no_target")),
                },
            }
            return out;
        }
        if input.fizzled {
            self.fizzled(&p, &mut out);
        }
        self.trip_frame(&p, input.idle, &mut out);
        out
    }

    /// One frame of carrying the journey out.
    fn trip_frame(&mut self, p: &Position, idle: bool, out: &mut Vec<TravelCommand>) {
        let Some(step) = self.trip.as_ref().and_then(|t| t.step.clone()) else {
            return;
        };
        match step {
            // Standing while the journey is planned.
            Step::Planning { .. } => return,
            Step::Ready { .. } => {
                self.ready(p, out);
                return;
            }
            Step::Ask => {
                match self.answer {
                    Some(true) => {
                        tracing::info!(target: "travel", "the player said yes");
                        if let Some(t) = self.trip.as_mut() {
                            t.confirmed = true;
                            t.step = None;
                        }
                        self.start_leg(p, out);
                    }
                    Some(false) => self.end(out, Some(p), Some("declined")),
                    None => {}
                }
                return;
            }
            Step::HandBack {
                ref hand_back,
                tries,
            } if tries.begun() || global_distance(p, &hand_back.at) <= HAND_BACK_REACH => {
                self.hand_back(p, out);
                return;
            }
            Step::Wait { what, watch } => {
                self.wait_frame(p, idle, &what, &watch, out);
                return;
            }
            Step::Approach { portal } if global_distance(p, &portal) < PORTAL_REACH => {
                self.use_portal(p, &portal, out);
                return;
            }
            _ => {}
        }
        let dirty = self.trip.as_mut().and_then(|t| t.route_dirty.take());
        if let Some(why) = dirty {
            tracing::info!(target: "travel", "replanning round {why}");
            let creature = self.obstacles.iter().any(|o| {
                o.obstacle.kind == ObstacleKind::Creature
                    && why.ends_with(&format!("{:08X}", o.obstacle.id.0))
            });
            if let Some(t) = self.trip.as_mut() {
                if creature {
                    t.stats.creature_replans += 1;
                } else {
                    t.stats.obstacle_replans += 1;
                }
            }
            self.replan_leg(p, out);
        }
        self.follow(p, out);
    }

    fn wait_frame(
        &mut self,
        p: &Position,
        idle: bool,
        what: &Awaited,
        watch: &TeleportWatch,
        out: &mut Vec<TravelCommand>,
    ) {
        let since = watch.since;
        let outcome = watch.update(p, idle, self.wall);
        // Every ten seconds of waiting, say what the wait sees.
        if ((self.wall - since) / 10.0).floor() > ((self.wall_before - since) / 10.0).floor() {
            tracing::info!(
                target: "travel",
                "waiting {:.0} s: idle {idle}, at {:08X}",
                self.wall - since,
                p.cell.0
            );
        }
        let id = self.trip.as_ref().map_or(0, |t| t.id);
        match outcome {
            TeleportOutcome::Waiting => {}
            TeleportOutcome::Landed(p) => {
                let t = unix_now();
                let name = self.character.name.clone();
                match what {
                    Awaited::Portal(s) => {
                        tracing::info!(
                            target: "travel",
                            "portal {:?} sent the character to {:08X} {:?}; learned",
                            s.name,
                            p.cell.0,
                            p.frame.origin
                        );
                        self.portals.learn_use(s, &p, t);
                        // Portal Recall sends a character to where the last portal it used sent it.
                        self.portals
                            .learn_recall(&name, Recall::Spell(PORTAL_RECALL), &p, t);
                        self.events.push(TravelEvent::Learned {
                            id,
                            what: Learned::Portal {
                                name: s.name.clone(),
                                to: p,
                            },
                        });
                        if let Some(trip) = self.trip.as_mut() {
                            trip.stats.portal_uses += 1;
                        }
                    }
                    Awaited::Recall(recall) => {
                        tracing::info!(
                            target: "travel",
                            "recall {} sent the character to {:08X} {:?}; learned",
                            recall.word(),
                            p.cell.0,
                            p.frame.origin
                        );
                        self.portals.learn_recall(&name, *recall, &p, t);
                        self.events.push(TravelEvent::Learned {
                            id,
                            what: Learned::Recall {
                                recall: *recall,
                                to: p,
                            },
                        });
                        if let Some(trip) = self.trip.as_mut() {
                            trip.stats.recalls += 1;
                            if std::mem::take(&mut trip.leave_stance) && self.stance.magic {
                                out.push(TravelCommand::ToggleCombat);
                            }
                        }
                    }
                    Awaited::WalkedInto(s) => {
                        tracing::info!(
                            target: "travel",
                            "walked into portal {:?}, which sent the character to {:08X} {:?}; \
                             learned, planning again",
                            s.name,
                            p.cell.0,
                            p.frame.origin
                        );
                        self.portals.learn_use(s, &p, t);
                        self.portals
                            .learn_recall(&name, Recall::Spell(PORTAL_RECALL), &p, t);
                        self.save_store();
                        self.events.push(TravelEvent::Learned {
                            id,
                            what: Learned::Portal {
                                name: s.name.clone(),
                                to: p,
                            },
                        });
                        if let Some(trip) = self.trip.as_mut() {
                            trip.step = None;
                            trip.stats.portal_uses += 1;
                        }
                        self.carry_on(&p, out, Some(format!("walked into portal {}", s.name)));
                        return;
                    }
                    Awaited::HandBack(name) => {
                        tracing::info!(
                            target: "travel",
                            "{name} sent the character to {:08X} {:?}",
                            p.cell.0,
                            p.frame.origin
                        );
                    }
                    Awaited::Elsewhere => {
                        tracing::info!(
                            target: "travel",
                            "taken to {:08X} {:?}; planning again",
                            p.cell.0,
                            p.frame.origin
                        );
                        if let Some(trip) = self.trip.as_mut() {
                            trip.step = None;
                            trip.stats.replans += 1;
                        }
                        self.carry_on(&p, out, Some(String::from("taken elsewhere")));
                        return;
                    }
                }
                self.save_store();
                let purpose = match self.trip.as_mut() {
                    Some(trip) => {
                        trip.step = None;
                        trip.purpose
                    }
                    None => return,
                };
                if purpose == Purpose::LearnRecalls {
                    self.next_recall(&p, out);
                    return;
                }
                if let Some(trip) = self.trip.as_mut() {
                    trip.leg += 1;
                }
                self.start_leg(&p, out);
            }
            TeleportOutcome::Refused => {
                let reason = format!("no teleport within {:.0} s of the use", self.wall - since);
                match what {
                    Awaited::Portal(s) => {
                        tracing::warn!(
                            target: "travel",
                            "portal {:?} refused the character ({reason}); retired",
                            s.name
                        );
                        self.portals.learn_refusal(s, &reason, unix_now());
                        self.save_store();
                        self.events.push(TravelEvent::Learned {
                            id,
                            what: Learned::Refusal {
                                name: s.name.clone(),
                                reason: reason.clone(),
                            },
                        });
                        let Some(trip) = self.trip.as_mut() else {
                            return;
                        };
                        trip.stats.refusals += 1;
                        trip.stats.replans += 1;
                        trip.step = None;
                        if trip.stats.replans > MAX_REPLANS {
                            self.end(out, Some(p), Some("too_many_replans"));
                            return;
                        }
                        self.carry_on(p, out, Some(format!("portal {} refused", s.name)));
                    }
                    Awaited::Recall(recall) => self.recall_failed(p, *recall, &reason, out),
                    Awaited::HandBack(name) => {
                        tracing::warn!(target: "travel", "{name} sent the character nowhere ({reason})");
                        self.end(out, Some(p), Some("not_sent_on"));
                    }
                    Awaited::WalkedInto(_) | Awaited::Elsewhere => {
                        if let Some(trip) = self.trip.as_mut() {
                            trip.step = None;
                        }
                        self.carry_on(p, out, Some(String::from("taken elsewhere")));
                    }
                }
            }
        }
    }

    /// A cast fizzled: while waiting on a recall spell, cast it again, or give it up after
    /// [`MAX_CASTS`].
    fn fizzled(&mut self, p: &Position, out: &mut Vec<TravelCommand>) {
        let Some(t) = self.trip.as_ref() else { return };
        let Some(Step::Wait {
            what: Awaited::Recall(recall @ Recall::Spell(_)),
            ..
        }) = t.step.clone()
        else {
            return;
        };
        if t.casts >= MAX_CASTS {
            self.recall_failed(p, recall, "fizzled", out);
            return;
        }
        tracing::info!(target: "travel", "the cast fizzled; casting again");
        self.set_step(Step::Ready {
            recall,
            since: self.wall,
            wielded: true,
            toggled: false,
        });
    }

    /// Use the portal standing at `portal`, or learn that none stands there.
    fn use_portal(&mut self, p: &Position, portal: &Position, out: &mut Vec<TravelCommand>) {
        self.release_keys(out);
        if let Some(t) = self.trip.as_mut() {
            t.follower = None;
        }
        let seen = self.portal_near(portal, 8.0).cloned();
        let Some(seen) = seen else {
            let sighting = PortalSighting {
                wcid: None,
                name: String::from("Portal"),
                position: *portal,
            };
            tracing::warn!(
                target: "travel",
                "no portal stands at {:?}; retired",
                portal.frame.origin
            );
            self.portals
                .learn_refusal(&sighting, "no portal stands there", unix_now());
            self.save_store();
            if let Some(t) = self.trip.as_mut() {
                t.stats.refusals += 1;
                t.step = None;
            }
            self.plan_trip(p, out, Some(String::from("no portal stands there")));
            return;
        };
        tracing::info!(
            target: "travel",
            "using portal {:?} {:08X}",
            seen.sighting.name,
            seen.id.0
        );
        out.push(TravelCommand::Use(seen.id));
        let wait = self.wait(Awaited::Portal(seen.sighting), p);
        self.set_step(wait);
    }

    /// Plan the current walking leg again from `p`.
    fn replan_leg(&mut self, p: &Position, out: &mut Vec<TravelCommand>) {
        match self.trip.as_ref().and_then(|t| t.step.clone()) {
            Some(Step::Walk { target, .. }) => self.walk_toward(p, &target, out),
            Some(Step::Approach { portal }) => self.walk_toward(p, &portal, out),
            Some(Step::HandBack { hand_back, .. }) => self.walk_toward(p, &hand_back.at, out),
            _ => {}
        }
    }

    /// Halfway along a route that ends at a window's edge, ask for the route on from its end,
    /// so it is there when the body is.
    fn ask_ahead(&mut self) {
        let Some(t) = self.trip.as_ref() else { return };
        let (
            Some(Step::Walk {
                target,
                last: false,
            }),
            Some(f),
            None,
        ) = (t.step.as_ref(), t.follower.as_ref(), t.ahead.as_ref())
        else {
            return;
        };
        let w = &f.path().waypoints;
        if f.next_waypoint() * 2 < w.len() {
            return;
        }
        let (Some(end), target) = (w.last().map(|w| w.position), *target) else {
            return;
        };
        let id = self.job();
        let Some(planning) = self.planning.as_mut() else {
            return;
        };
        planning.ask(Job::Route {
            id,
            from: end,
            to: Goal::Place(target),
            reach: None,
            snap: Some(6.0),
        });
        if let Some(t) = self.trip.as_mut() {
            t.ahead = Some((id, None));
        }
    }

    #[allow(clippy::too_many_lines)]
    fn follow(&mut self, p: &Position, out: &mut Vec<TravelCommand>) {
        self.ask_ahead();
        let now = self.now;
        let wall = self.wall;
        let Some(t) = self.trip.as_mut() else { return };
        let Some(f) = t.follower.as_mut() else { return };
        let facing = dereth_primitives::frame::get_heading(&p.frame);
        match f.update(p, facing, now) {
            Steer::Arrived => {
                t.follower = None;
                match t.step.clone() {
                    Some(Step::Walk { target, last }) => {
                        if last {
                            t.leg += 1;
                            self.start_leg(p, out);
                        } else {
                            // At a window's edge: on by the route asked for on the way here.
                            match t.ahead.take() {
                                Some((_, Some(Ok((path, last))))) => self.follow_route(path, last),
                                Some((job, None)) => {
                                    self.release_keys(out);
                                    if let Some(t) = self.trip.as_mut() {
                                        t.walking = Some((job, target));
                                    }
                                }
                                _ => self.walk_toward(p, &target, out),
                            }
                        }
                    }
                    Some(Step::Approach { portal }) => {
                        if global_distance(p, &portal) < PORTAL_REACH + 3.0 {
                            self.use_portal(p, &portal, out);
                        } else {
                            self.walk_toward(p, &portal, out);
                        }
                    }
                    Some(Step::HandBack { hand_back, .. }) => {
                        if global_distance(p, &hand_back.at) < HAND_BACK_REACH + 3.0 {
                            self.hand_back(p, out);
                        } else {
                            self.walk_toward(p, &hand_back.at, out);
                        }
                    }
                    _ => {}
                }
            }
            Steer::Stuck { waypoint } => {
                // Where the body stopped is no way through, whatever the floor says: the route is
                // planned again round it.
                let toward = f.path().waypoints.get(waypoint).map(|w| w.position);
                if let (Some(planning), Some(toward)) = (self.planning.as_mut(), toward) {
                    planning.ask(Job::Block { at: *p, toward });
                }
                t.stats.stuck += 1;
                let give_up = t.stats.replans >= MAX_REPLANS;
                if !give_up {
                    t.stats.replans += 1;
                }
                tracing::warn!(
                    target: "travel",
                    "stuck at {:08X} {:?} facing {:.0} on the way to waypoint {waypoint}; \
                     near: {:?}",
                    p.cell.0,
                    p.frame.origin,
                    dereth_primitives::frame::get_heading(&p.frame),
                    self.near(p, 4.0)
                );
                if give_up {
                    self.end(out, Some(p), Some("stuck"));
                    return;
                }
                self.replan_leg(p, out);
            }
            Steer::OpenDoor(door) => {
                let open = self.obstacles.iter().any(|o| {
                    o.obstacle.id == door && o.obstacle.kind == ObstacleKind::Door(DoorState::Open)
                });
                let answered = t.lock_answers.contains(&door.0);
                let used = t
                    .door
                    .filter(|(d, ..)| *d == door)
                    .map(|(_, since, tries)| (since, tries));
                match at_door(open, answered, used, wall) {
                    AtDoor::Through => {
                        tracing::info!(target: "travel", "door {:08X} is open", door.0);
                        f.door_opened();
                        t.door = None;
                    }
                    AtDoor::Wait => {}
                    AtDoor::Locked | AtDoor::Blocked => {
                        if answered {
                            tracing::warn!(
                                target: "travel",
                                "door {:08X} is locked, the server says; planning round it",
                                door.0
                            );
                        } else {
                            tracing::warn!(
                                target: "travel",
                                "door {:08X} did not open after {DOOR_TRIES} uses; planning round it for now",
                                door.0
                            );
                        }
                        t.door = None;
                        t.locked.insert(door.0);
                        t.route_dirty = None;
                        t.stats.replans += 1;
                        let list = std::mem::take(&mut self.obstacles);
                        self.set_obstacles(list, Some(p));
                        self.replan_leg(p, out);
                    }
                    AtDoor::Use => {
                        let tries = used.map_or(0, |(_, n)| n) + 1;
                        t.door = Some((door, wall, tries));
                        if tries == 1 {
                            tracing::info!(target: "travel", "opening door {:08X}", door.0);
                            t.stats.doors += 1;
                            let (id, index) = (t.id, t.leg);
                            let at = self
                                .obstacles
                                .iter()
                                .find(|o| o.obstacle.id == door)
                                .map_or(*p, |o| o.obstacle.position);
                            self.events.push(TravelEvent::Leg {
                                id,
                                index,
                                leg: TravelLeg::Door { object: door, at },
                            });
                        } else {
                            tracing::info!(
                                target: "travel",
                                "door {:08X} has not opened; using it again ({tries} of {DOOR_TRIES})",
                                door.0
                            );
                        }
                        if self.forward {
                            out.push(TravelCommand::Forward(false));
                            self.forward = false;
                        }
                        out.push(TravelCommand::Use(door));
                    }
                }
            }
            Steer::Go {
                forward: go, turn, ..
            } => {
                if turn != self.turning {
                    out.push(TravelCommand::Turn(turn));
                    self.turning = turn;
                }
                if go != self.forward {
                    out.push(TravelCommand::Forward(go));
                    self.forward = go;
                }
            }
        }
    }
}

impl Trip {
    fn new(
        id: u64,
        purpose: Purpose,
        target: TravelTarget,
        allow: TravelAllow,
        now: f64,
        wall: f64,
    ) -> Self {
        Self {
            id,
            purpose,
            target,
            goal: None,
            allow,
            journey: None,
            view: Vec::new(),
            leg: 0,
            step: None,
            follower: None,
            door: None,
            locked: BTreeSet::new(),
            lock_answers: BTreeSet::new(),
            disabled: BTreeSet::new(),
            route_dirty: None,
            stats: TripStats::default(),
            started: now,
            started_wall: wall,
            last: None,
            leave_stance: false,
            casts: 0,
            confirmed: false,
            walking: None,
            unblocked: false,
            ahead: None,
        }
    }
}

/// A walk to `to` as shown: its route's corners, with each door on the way a leg of its own.
fn walk_legs(to: Position, path: Option<&NavPath>) -> Vec<TravelLeg> {
    let Some(path) = path else {
        return vec![TravelLeg::Walk {
            to,
            path: Vec::new(),
        }];
    };
    let mut out = Vec::new();
    let mut corners = Vec::new();
    for w in &path.waypoints {
        corners.push(w.position);
        if let Some(WaypointAction::OpenDoor(object)) = w.action {
            out.push(TravelLeg::Walk {
                to: w.position,
                path: std::mem::take(&mut corners),
            });
            out.push(TravelLeg::Door {
                object,
                at: w.position,
            });
            corners.push(w.position);
        }
    }
    out.push(TravelLeg::Walk { to, path: corners });
    out
}

/// A journey in a line, for the log.
fn describe(j: &Journey) -> String {
    j.legs
        .iter()
        .map(|l| match l {
            Leg::Walk { to } => format!("walk to {:08X}", to.cell.0),
            Leg::Portal { edge } => format!(
                "portal {:?} ({:?}) to {:?}",
                edge.name,
                edge.source,
                edge.to.map(|t| t.cell)
            ),
            Leg::Teleport { teleport } => format!(
                "recall {:?} ({}) to {:08X}",
                teleport.name,
                teleport.recall.word(),
                teleport.to.cell.0
            ),
            Leg::HandBack { hand_back } => format!(
                "hand the {:?} back to {:?} to {:08X}",
                hand_back.token, hand_back.name, hand_back.to.cell.0
            ),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The place the most steps from `start` across the region's walkable places.
#[must_use]
pub fn farthest_by_walking(region: &dereth_navigation::Region, start: u32) -> u32 {
    let mut seen = std::collections::HashSet::from([start]);
    let mut frontier = vec![start];
    let mut last = start;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for i in frontier {
            last = i;
            for j in region.neighbours(i) {
                if seen.insert(j) {
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    last
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the travel's own bookkeeping).
    use super::*;

    #[test]
    fn a_world_name_becomes_a_safe_file_name() {
        assert_eq!(
            file_name("Navigation proof@127.0.0.1:19340"),
            "Navigation_proof_127_0_0_1_19340"
        );
    }

    #[test]
    fn a_published_table_is_the_servers_knowledge_in_every_world_store_and_never_saved() {
        let mut t = Travel::new(StoreFiles::default());
        let text = "dereth-navigation-store\t1\nworld\tW\n\
            portal\tpublished\t2068\tDrudge Hideout\t0xA9B30002 5.4 29.5 94.0\t0x019E0114 10.0 -40.0 0.0\t0\t0\t0\t-\t-\t-\n\
            portal\thint\t1\tOther\t0xA9B30002 50.0 29.5 94.0\t0x019E0114 10.0 -40.0 0.0\t0\t0\t0\t-\t-\t-\n";
        assert_eq!(t.set_published(text), Ok(1), "only the published edges");
        t.load_store("A world");
        let store = t.store();
        assert_eq!(store.portals().len(), 1);
        assert_eq!(store.portals()[0].source, EdgeSource::Published);
        assert_eq!(store.routable(None).len(), 1);
        // What is saved for the world is what the character learned, nothing more.
        assert!(store
            .without(EdgeSource::Hint)
            .without(EdgeSource::Published)
            .portals()
            .is_empty());
        assert!(t.set_published("not a store").is_err());
    }

    #[test]
    fn a_refused_published_portal_stays_out_of_routes_in_the_next_session_kept_in_the_folder() {
        let dir =
            std::env::temp_dir().join(format!("dereth-travel-refusal-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let files = || StoreFiles {
            dir: Some(dir.clone()),
            prefix: String::from("navigation-"),
        };
        let table = "dereth-navigation-store\t1\nworld\tW\n\
            portal\tpublished\t28709\tDestroyed Portal\t0x00020225 100.0 -40.0 -0.2\t0x33D90015 59.1 100.3 52.0\t0\t0\t0\t-\t-\t-\n";
        let mut first = Travel::new(files());
        assert_eq!(first.set_published(table), Ok(1));
        first.load_store("Litmus@127.0.0.1:9800");
        assert_eq!(first.store().routable(None).len(), 1);
        let edge = first.store().portals()[0].clone();
        let sighting = PortalSighting {
            wcid: edge.wcid,
            name: edge.name.clone(),
            position: edge.from,
        };
        first
            .portals
            .learn_refusal(&sighting, "no teleport after the use", 1);
        first.save_store();
        // The next session, a new client over the same folder: the refusal is read back and the
        // server's table taken in again, and the portal stays out of routes.
        let mut next = Travel::new(files());
        assert_eq!(next.set_published(table), Ok(1));
        next.load_store("Litmus@127.0.0.1:9800");
        assert!(next.store().routable(None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_door_is_judged_locked_only_on_the_servers_answer_and_a_slow_one_is_used_again() {
        // Shut and not yet used: use it. Open: through.
        assert_eq!(at_door(false, false, None, 10.0), AtDoor::Use);
        assert_eq!(at_door(true, false, Some((10.0, 1)), 11.0), AtDoor::Through);
        // Used, and the wait not over: wait.
        assert_eq!(at_door(false, false, Some((10.0, 1)), 13.0), AtDoor::Wait);
        // The wait over and still shut, with no word from the server: used again, up to the
        // tries, then round it for now.
        assert_eq!(at_door(false, false, Some((10.0, 1)), 14.5), AtDoor::Use);
        assert_eq!(
            at_door(false, false, Some((10.0, DOOR_TRIES - 1)), 14.5),
            AtDoor::Use
        );
        assert_eq!(
            at_door(false, false, Some((10.0, DOOR_TRIES)), 14.5),
            AtDoor::Blocked
        );
        // The server said it is locked: round it at once, however few the tries.
        assert_eq!(at_door(false, true, Some((10.0, 1)), 10.5), AtDoor::Locked);
        assert_eq!(at_door(false, true, None, 10.0), AtDoor::Locked);
    }

    #[test]
    fn a_walk_with_a_door_on_the_way_shows_as_walk_door_walk() {
        let at = |x: f32| {
            Position::new(
                CellId(0x0106_0104),
                Frame::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY),
            )
        };
        let w = |x: f32, door: Option<u32>| dereth_navigation::Waypoint {
            position: at(x),
            action: door.map(|d| WaypointAction::OpenDoor(ObjectId(d))),
        };
        let path = NavPath {
            waypoints: vec![w(0.0, None), w(5.0, Some(0x7A9B_401A)), w(9.0, None)],
            legs: Vec::new(),
            goal_offset: 0.0,
            cuts: Vec::new(),
        };
        let legs = walk_legs(at(9.0), Some(&path));
        let kinds: Vec<&str> = legs.iter().map(TravelLeg::kind).collect();
        assert_eq!(kinds, ["walk", "door", "walk"]);
        assert_eq!(
            legs[1],
            TravelLeg::Door {
                object: ObjectId(0x7A9B_401A),
                at: at(5.0)
            }
        );
        let TravelLeg::Walk { path, .. } = &legs[2] else {
            panic!("a walk")
        };
        assert_eq!(path, &vec![at(5.0), at(9.0)]);
    }

    #[test]
    fn a_hand_back_uses_the_person_waits_for_the_token_then_gives_it_back_until_it_is_taken() {
        let token = ObjectId(0x8000_0232);
        let mut tries = HandBackTries::default();
        // No token: the person is used, then waited on, then used again.
        assert_eq!(tries.next(None, 1.0), HandBackMove::Use(1));
        assert_eq!(tries.next(None, 1.0 + TOKEN_WAIT - 0.1), HandBackMove::Wait);
        assert_eq!(tries.next(None, 1.0 + TOKEN_WAIT), HandBackMove::Use(2));
        // The token comes: held a moment first, while the person finishes talking.
        let came = 20.0;
        assert_eq!(tries.next(Some(token), came), HandBackMove::Wait);
        assert_eq!(
            tries.next(Some(token), came + TOKEN_SETTLE - 0.1),
            HandBackMove::Wait
        );
        assert_eq!(
            tries.next(Some(token), came + TOKEN_SETTLE),
            HandBackMove::Give(token, 1)
        );
        // Still in the pack: given again only after a while, and not without end.
        let first = came + TOKEN_SETTLE;
        assert_eq!(tries.next(Some(token), first + 1.0), HandBackMove::Wait);
        assert_eq!(
            tries.next(Some(token), first + GIVE_WAIT),
            HandBackMove::Give(token, 2)
        );
        // Gone from the pack after a give: taken.
        assert_eq!(
            tries.next(None, first + GIVE_WAIT + 1.0),
            HandBackMove::Taken
        );
        // Given every time and never taken: given up.
        let mut stubborn = HandBackTries::default();
        let mut now = 0.0;
        let mut gives = 0;
        loop {
            match stubborn.next(Some(token), now) {
                HandBackMove::Give(..) => gives += 1,
                HandBackMove::NotTaken(n) => {
                    assert_eq!((n, gives), (GIVES, GIVES));
                    break;
                }
                HandBackMove::Wait => {}
                other => panic!("{other:?}"),
            }
            now += 0.5;
        }
        // No token in all the uses: given up.
        let mut empty = HandBackTries::default();
        let mut now = 0.0;
        loop {
            match empty.next(None, now) {
                HandBackMove::NoToken(n) => {
                    assert_eq!(n, TOKEN_USES);
                    break;
                }
                HandBackMove::Use(_) | HandBackMove::Wait => {}
                other => panic!("{other:?}"),
            }
            now += 0.5;
        }
    }

    #[test]
    fn a_hand_back_sends_where_a_portal_out_of_the_persons_dungeon_sends() {
        let place = |cell: u32, x: f32, y: f32| {
            Position::new(
                CellId(cell),
                Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
            )
        };
        let mut t = Travel::new(StoreFiles::default());
        let text = "dereth-navigation-store\t1\nworld\tW\n\
            portal\tpublished\t29338\tExit to Holtburg\t0x86020169 158.641 -149.516 -6.063\t0xA9B40019 84.0 7.1 94.0\t0\t0\t0\t-\t-\t-\n\
            portal\tpublished\t1\tWithin\t0x86020273 70.0 -40.0 0.0\t0x8602021E 50.0 -54.0 0.0\t0\t0\t0\t-\t-\t-\n\
            portal\tpublished\t2\tElsewhere\t0x7F030169 158.641 -149.516 -6.063\t0xDA55001D 84.8 99.0 20.0\t0\t0\t0\t-\t-\t-\n";
        assert_eq!(t.set_published(text), Ok(3));
        t.load_store("W");
        let jonathan = HandBackSeen {
            object: ObjectId(0x8000_0116),
            name: String::from("Jonathan"),
            at: place(0x8602_01B0, 22.1, -19.1),
            token: String::from("Academy Exit Token"),
            question: String::from("Leave?"),
        };
        t.hand_backs = vec![jonathan.clone()];
        let offered = t.hand_backs();
        assert_eq!(offered.len(), 1);
        // The exit of his own dungeon; not the portal that stays inside it, nor another's.
        assert_eq!(offered[0].to.cell, CellId(0xA9B4_0019));
        assert_eq!(offered[0].object, Some(jonathan.object));
        // A dungeon whose way out is not known offers no hand-back.
        t.hand_backs = vec![HandBackSeen {
            at: place(0x8C04_01B0, 22.1, -19.1),
            ..jonathan
        }];
        assert!(t.hand_backs().is_empty());
    }

    /// Frames of the travel answering a walking route over the retail data: a dungeon's region
    /// built for the first route there, a window of landblocks round two places a landblock
    /// apart, and a long winding walk over hills. Each frame is the travel's whole step, as the
    /// client runs it once a frame.
    mod frames {
        use super::*;

        pub(super) struct Scene {
            pub name: &'static str,
            pub from: Position,
            pub to: Position,
        }

        fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
            Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
        }

        /// A place on the ground of landblock `block`.
        fn ground(land: &DatLandSource, block: u16, x: f32, y: f32) -> Position {
            let z = land
                .ground_height(LandblockId(block), x, y)
                .expect("the landblock has terrain");
            let mut cell = CellId((u32::from(block) << 16) | 1);
            let mut o = Vec3::new(x, y, z);
            assert!(dereth_physics::landdefs::adjust_to_outside(
                &mut cell, &mut o
            ));
            Position::new(cell, Frame::new(o, Quat::IDENTITY))
        }

        pub(super) fn scenes(land: &DatLandSource) -> Vec<Scene> {
            vec![
                Scene {
                    name: "the Academy's outer courtyard to the hive's floor",
                    from: at(0x8602_02C3, 119.0, -141.0, 0.0),
                    to: at(0x8602_0101, 148.5, -236.75, -11.1),
                },
                Scene {
                    name: "Holtburg to the landblock north of it",
                    from: ground(land, 0xA9B4, 20.0, 20.0),
                    to: ground(land, 0xA9B5, 150.0, 120.0),
                },
                Scene {
                    name: "across the hills",
                    from: ground(land, 0xB5BF, 20.0, 96.0),
                    to: ground(land, 0xB5BF, 170.0, 96.0),
                },
            ]
        }

        /// What answering one route cost the frames: how many frames it took, the longest of
        /// them and their sum, in milliseconds, and the answer.
        pub(super) struct Cost {
            pub frames: u32,
            pub worst_ms: f64,
            pub total_ms: f64,
            pub route: Option<Result<TravelRoute, String>>,
        }

        /// The route of `scene` asked for and answered, frame by frame; with `paced`, each frame
        /// waits a moment before the next, as a client's do, while the planning's own thread works.
        pub(super) fn answer(
            travel: &mut Travel,
            land: &Arc<DatLandSource>,
            store: &Arc<RetailDatStore>,
            scene: &Scene,
            id: u64,
            paced: bool,
        ) -> Cost {
            travel.request(TravelRequest::Route {
                id,
                target: TravelTarget::Place(scene.to),
            });
            let mut cost = Cost {
                frames: 0,
                worst_ms: 0.0,
                total_ms: 0.0,
                route: None,
            };
            let object = |_: ObjectId| None;
            let holds = |_: &str| None;
            let started = web_time::Instant::now();
            while cost.route.is_none() && started.elapsed().as_secs_f64() < 60.0 {
                let input = TravelInput {
                    now: f64::from(cost.frames) / 30.0,
                    wall: f64::from(cost.frames) / 30.0,
                    at: Some(scene.from),
                    idle: true,
                    land: Some(land),
                    store,
                    character: CharacterState::default(),
                    stance: Stance::default(),
                    portals: Vec::new(),
                    world: Some(String::from("frames")),
                    player_input: false,
                    fizzled: false,
                    object: &object,
                    hand_backs: Vec::new(),
                    holds: &holds,
                    answer: None,
                };
                let t = web_time::Instant::now();
                let _ = travel.use_time(&input);
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                cost.frames += 1;
                cost.worst_ms = cost.worst_ms.max(ms);
                cost.total_ms += ms;
                for e in travel.take_events() {
                    if let TravelEvent::Route { id: got, route } = e {
                        if got == id {
                            cost.route = Some(route);
                        }
                    }
                }
                if paced {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
            cost
        }
    }

    /// Behaviour: plugins.nav.planning-never-holds-a-frame
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_route_through_a_window_of_landblocks_never_built_is_answered_without_holding_a_frame() {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let scenes = |land: &DatLandSource| frames::scenes(land).remove(1);
        // On a thread of its own: the frames go on as the window is built and the route planned,
        // seconds of work that once held one frame.
        let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land"));
        let mut travel = Travel::new(StoreFiles::default());
        let scene = scenes(&land);
        let threaded = frames::answer(&mut travel, &land, &store, &scene, 1, true);
        let route = threaded.route.expect("answered").expect("a route");
        assert!(route.length > 100.0, "{}", route.length);
        assert!(
            threaded.frames > 1,
            "answered in the frame that asked: {} frames",
            threaded.frames
        );
        // A step of each frame, where there is no thread: the same route, a landblock's cells or
        // a slice of the search at a time, spread over frames.
        let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land"));
        let mut travel = Travel::new(StoreFiles::default());
        travel.planning = Some(
            Planning::in_frame(&land, &store)
                .expect("a planner")
                .one_step_a_frame(),
        );
        let sliced = frames::answer(&mut travel, &land, &store, &scenes(&land), 1, false);
        assert_eq!(sliced.route, Some(Ok(route)));
        assert!(sliced.frames > 50, "{} frames", sliced.frames);
    }

    #[test]
    #[ignore = "instrument: prints the frames a walking route costs, over the retail dats"]
    fn frames_a_walking_route_costs() {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        println!("| planning | route | frames | worst frame ms | frames' sum ms | answered |");
        println!("|---|---|---|---|---|---|");
        for in_frame in [false, true] {
            let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land"));
            let mut travel = Travel::new(StoreFiles::default());
            if in_frame {
                travel.planning = Some(Planning::in_frame(&land, &store).expect("a planner"));
            }
            for (i, scene) in frames::scenes(&land).iter().enumerate() {
                let c = frames::answer(&mut travel, &land, &store, scene, i as u64 + 1, !in_frame);
                println!(
                    "| {} | {} | {} | {:.1} | {:.1} | {} |",
                    if in_frame {
                        "a slice of each frame"
                    } else {
                        "a thread"
                    },
                    scene.name,
                    c.frames,
                    c.worst_ms,
                    c.total_ms,
                    match &c.route {
                        Some(Ok(r)) => format!("{:.0} m", r.length),
                        Some(Err(e)) => e.clone(),
                        None => String::from("not in a minute"),
                    }
                );
            }
        }
    }
}
