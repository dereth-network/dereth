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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dereth_dat::RetailDatStore;
use dereth_navigation::reach;
use dereth_navigation::{
    global, global_distance, CharacterState, DoorState, EdgeSource, FollowConfig, Follower,
    HandBack, Journey, LandNavSource, Leg, NavConfig, NavPath, NavWorld, NoPath, Obstacle,
    ObstacleKind, PortalShape, PortalSighting, PortalStore, Recall, StaticPlacement, Steer,
    Teleport, TeleportOutcome, TeleportWatch, WaypointAction,
};
use dereth_physics::SetupGeometry;
use dereth_primitives::num::math::hypotf;
use dereth_primitives::num::to_i32;
use dereth_primitives::{CellId, DataId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};
use dereth_world_data::land_source::DatLandSource;

use crate::planning::{Answer, Goal, Job, Planning};

mod places;
pub use places::PlaceSeen;
mod published;
pub use published::{
    published_documents, Answered, Fetched, PublishedReading, PUBLISHED_AGAIN, PUBLISHED_TRIES,
};

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
    /// Handing a token back (leaving the Training Academy for good): who says yes to it.
    pub hand_backs: HandBacks,
}

impl Default for TravelAllow {
    fn default() -> Self {
        Self {
            portals: true,
            spells: true,
            items: true,
            commands: true,
            hand_backs: HandBacks::Ask,
        }
    }
}

/// Who agrees to a journey's handing a token back, which cannot be undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandBacks {
    /// The player is asked, in the client's own yes-or-no box: their own journeys.
    Ask,
    /// Agreed already, by whoever asked for the journey: it is carried out unasked.
    Agreed,
    /// Nobody can agree (a plugin's journey that has not): the journey ends at once, saying why.
    Refused,
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
    /// Step off the ledge at `from` and come down at `to`, `height` metres below.
    Drop {
        from: Position,
        to: Position,
        height: f32,
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
            Self::Drop { .. } => "drop",
        }
    }

    /// Where the leg ends, when it is known.
    #[must_use]
    pub fn end(&self) -> Option<Position> {
        match self {
            Self::Walk { to, .. } => Some(*to),
            Self::Door { at, .. } => Some(*at),
            Self::Portal { to, .. } => *to,
            Self::Recall { to, .. } | Self::HandBack { to, .. } | Self::Drop { to, .. } => {
                Some(*to)
            }
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
            Leg::Drop { drop } => Self::Drop {
                from: drop.stand,
                to: drop.land,
                height: drop.height,
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

/// A door a journey could not get through, and why it stayed shut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutDoor {
    pub object: ObjectId,
    pub why: DoorWhy,
}

/// Why a door stayed shut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorWhy {
    /// The server answered its use as locked.
    Locked,
    /// Used and used again, it did not open, and no answer said why.
    NoAnswer,
    /// No walk reaches any of the levers that work it.
    NoLever,
    /// Its lever was worked, and it did not open.
    LeverFailed,
}

impl DoorWhy {
    /// The reason as one word, as plugins read it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::NoAnswer => "no_answer",
            Self::NoLever => "no_lever",
            Self::LeverFailed => "lever_failed",
        }
    }
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
    /// The journey ended without arriving; `door` when a door on the way would not open.
    Failed {
        id: u64,
        reason: String,
        stats: TripStats,
        door: Option<ShutDoor>,
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
    /// Hold or release the walk key: forward walks rather than runs while it is held.
    Walk(bool),
    /// Jump, the least a jump can: the key pressed and let go at once.
    Jump,
    /// Hold the turn-left key (`Some(false)`), the turn-right key (`Some(true)`), or neither.
    Turn(Option<bool>),
    /// Face this heading at once, degrees clockwise from north: the last of a turn, less than a
    /// turn key held for one frame would turn.
    Face(f32),
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
    /// Why a journey that hands it back ends when nobody can agree to it.
    pub refused: String,
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
    /// A portal's refusal the server sent since the last step ([`PORTAL_REFUSALS`]).
    pub portal_refused: Option<u32>,
    /// Where an object is.
    pub object: &'a dyn Fn(ObjectId) -> Option<Position>,
    /// The people in sight who send the character on for a token handed back.
    pub hand_backs: Vec<HandBackSeen>,
    /// An item of this name the character holds, if any.
    pub holds: &'a dyn Fn(&str) -> Option<ObjectId>,
    /// The player's answer to the travel's question, since the last step.
    pub answer: Option<bool>,
    /// The character's body spheres, when it has a body ([`reach::body_of`]); empty for the
    /// human body's.
    pub body: Vec<reach::BodySphere>,
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
    /// Where the planner keeps the tiles and dungeons it builds ([`CacheTiles`]); `None` keeps
    /// none.
    pub tiles: Option<PathBuf>,
    /// A place package taken in as hints ([`HINT_PORTALS`], [`HINT_PLACES`]); `None` for none.
    pub hints: Option<PathBuf>,
}

/// The portal table of a place package ([`StoreFiles::hints`]).
pub const HINT_PORTALS: &str = "portals.tsv";
/// The places of a place package.
pub const HINT_PLACES: &str = "places.tsv";

impl StoreFiles {
    /// Where a client run on `cfg` keeps the travel's files: in `--nav-store`'s folder, or beside
    /// the preferences file.
    #[must_use]
    pub fn of(cfg: &crate::config::Config) -> Self {
        match &cfg.nav_store {
            Some(dir) => Self {
                dir: Some(dir.clone()),
                prefix: String::new(),
                tiles: cfg.nav_cache.clone(),
                hints: cfg.nav_hints.clone(),
            },
            None => Self {
                dir: cfg.preferences_file.parent().map(Path::to_path_buf),
                prefix: String::from("navigation-"),
                tiles: cfg.nav_cache.clone(),
                hints: cfg.nav_hints.clone(),
            },
        }
    }
}

/// The `useability` bit that says an object cannot be used (`USEABLE_NO`): the client sends no
/// use for it, so nothing answers one.
const USEABLE_NO: u32 = 0x0000_0001;

/// How a door stands for the walk: open, or closed and opened by a use, or one no use opens. A
/// door that says it cannot be used (a prison door a lever elsewhere opens) is planned round at
/// once: the client never sends a use for it, so no answer would ever say it is locked.
#[must_use]
pub fn door_state(open: bool, useability: Option<u32>) -> DoorState {
    if open {
        DoorState::Open
    } else if useability.is_some_and(|u| u & USEABLE_NO != 0) {
        DoorState::Locked
    } else {
        DoorState::Closed
    }
}

/// How far from a portal a walk that is not taking it ends when what the portal's body is is not
/// known, meters: a body that walks into a portal's shape is carried off by it, and a large
/// portal reaches a few meters out.
const PORTAL_KEEP_OFF: f32 = 6.0;

/// How much further than a portal's reach ([`reach::reach`]) a walk that is not taking it ends.
const PORTAL_MARGIN: f32 = reach::MARGIN;

/// How far from a route's end a portal it might walk onto is looked for, meters: beyond any
/// portal's reach.
const PORTAL_LOOK: f32 = 15.0;

/// How long a used door is waited for to stand open, seconds; and how many uses it gets before
/// the walk goes round it for now. A door is judged locked only on the server's answer. With no
/// way round, a door with no such answer gets this many uses once more, and that is the most a
/// journey gives it.
const DOOR_WAIT: f64 = 4.0;
const DOOR_TRIES: u32 = 4;

/// How near a portal stands to a place no walk reaches for it to be that portal, metres: the walk
/// aims at the nearest place to stand, which may be a few metres off the portal.
const UNWALKABLE_NEAR: f32 = 8.0;

/// What a journey fails with when the only portals that lead on stand where no walk reaches.
const UNWALKABLE: &str = "no walk reaches";

/// The most portals one trip learns no walk reaches before it gives up.
const MAX_UNWALKABLE: usize = 4;

/// What a journey fails with when its walk has no route and a door on the way would not open.
const DOOR_SHUT: &str = "door_shut";

/// How near the door a lever's place names a door stands to be that door, metres; and how near
/// the lever's place an object stands to be the lever.
const LEVER_NEAR: f32 = 1.5;

/// The levers that work the door standing at `door`, as the world's places name them.
fn levers_of(places: &dereth_navigation::PlaceStore, door: &Position) -> Vec<Position> {
    places
        .places()
        .iter()
        .filter(|p| {
            p.kind == dereth_navigation::PlaceKind::Lever
                && p.to.is_some_and(|d| global_distance(&d, door) < LEVER_NEAR)
        })
        .map(|p| p.at)
        .collect()
}

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
    /// At a door no use opens: walking to the `which`th of its `levers` (nearest first), then
    /// working it (`used`: when last, and how often) until the door stands open; then on with
    /// `resume`, the step the walk was on.
    Lever {
        door: ObjectId,
        levers: Vec<Position>,
        which: usize,
        used: Option<(f64, u32)>,
        resume: Box<Step>,
    },
    /// At a ledge: walking at it since `since` (wall seconds), hopping off it (`hops` so far),
    /// then, once `off`, falling until the body stops coming down (`still` steps at `last_z`).
    Drop {
        drop: dereth_navigation::DropLink,
        since: f64,
        hops: u32,
        off: bool,
        last_z: f32,
        still: u32,
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
    /// open after [`DOOR_TRIES`] uses (something in the way, or slow), which are tried again once
    /// when no other way remains.
    locked: BTreeSet<u32>,
    /// The last door on the way that would not open, named, and why: what a journey that finds
    /// no way round it says.
    shut: Option<String>,
    /// That door, and why it stayed shut.
    shut_door: Option<ShutDoor>,
    /// Doors whose use the server answered as locked (`OpenFailDueToLock`).
    lock_answers: BTreeSet<u32>,
    /// Doors that have had their second go: if they stay shut, the journey has no way through them.
    tried_again: BTreeSet<u32>,
    /// How many times the walk went to each door's levers (by door): a door that stays shut
    /// after [`DOOR_TRIES`] goes is planned round.
    lever_goes: BTreeMap<u32, u32>,
    /// Whether the journey was planned again because only a drop led on from a walk: once the
    /// walk's places are built, its floors no walk joins are told apart.
    drop_replanned: bool,
    /// Recalls that did not land on this trip: not offered again.
    disabled: BTreeSet<Recall>,
    /// Where portals stand that no walk reaches from the land the trip walked on (a portal on a
    /// plateau only other portals lead to, say): planned round on this trip.
    unwalkable: Vec<Position>,
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
/// How near a portal's destination a body set down is to have been sent by it, metres; and the
/// least a step must carry the body to be looked at as one.
const LANDS_WITHIN: f32 = 5.0;
/// An object whose own place a body cannot walk to (a shopkeeper behind a counter, someone in a
/// booth with a window to the street) is walked to within this of it: inside the distance an
/// object is used from.
pub const OBJECT_REACH: f32 = 3.0;
/// The spell that sends a character to where the last portal it used sent it.
pub const PORTAL_RECALL: u32 = 2645;
/// The server's word that a cast fizzled (`Communication_WeenieError` code).
pub const FIZZLED: u32 = 0x0402;

/// The server's words that a portal will not take the character (`Communication_WeenieError`
/// codes), each with whether the refusal lasts. One that lasts (a quest the portal asks, a level,
/// a player killer's standing, an expansion) is kept for the character; a passing one (teleported
/// or in a player killer battle too recently, weakened by vitae) is not.
pub const PORTAL_REFUSALS: [(u32, bool); 18] = [
    (0x0446, true),  // QuestSolvedTooLongAgo
    (0x045C, true),  // PKsMayNotUsePortal
    (0x045D, true),  // NonPKsMayNotUsePortal
    (0x0466, true),  // YouMustHaveDarkMajestyToUsePortal
    (0x0474, true),  // YouMustCompleteQuestToUsePortal
    (0x04A7, false), // YouHaveBeenTeleportedTooRecently
    (0x04A8, true),  // YouMustBeAnAdvocateToUsePortal
    (0x04AA, true),  // PlayersMayNotUsePortal
    (0x04AB, true),  // YouAreNotPowerfulEnoughToUsePortal
    (0x04AC, true),  // YouAreTooPowerfulToUsePortal
    (0x04C4, true),  // YouMustBeHouseGuestToUsePortal
    (0x04CC, false), // YouHaveBeenInPKBattleTooRecently
    (0x04F3, true),  // PKLiteMayNotUsePortal
    (0x0554, true),  // MustPurchaseThroneOfDestinyToUsePortal
    (0x058F, true),  // OnlyOlthoiMayUsePortal
    (0x0590, true),  // OlthoiMayNotUsePortal
    (0x0591, false), // YouMayNotUsePortalWithVitae
    (0x0592, true),  // YouMustBeTwoWeeksOldToUsePortal
];

/// What the server said in refusing a portal with `code`, as the chat shows it, and whether the
/// refusal lasts; `None` for a code that is no portal's refusal.
#[must_use]
pub fn portal_refusal(code: u32) -> Option<(String, bool)> {
    let &(_, lasting) = PORTAL_REFUSALS.iter().find(|(c, _)| *c == code)?;
    // The quest solved too long ago has no line of the client's own: the server says it in chat.
    let text = dereth_client_contract::chat::failure::arm_for(code)
        .and_then(|(_, arm)| arm.render(""))
        .unwrap_or_else(|| {
            String::from("You completed the quest this portal requires too long ago!")
        });
    Some((text.trim_end().to_owned(), lasting))
}
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

/// A cell's middle: the middle of an interior cell's largest floor, or of a land cell on the
/// ground. A cell with no floor of its own (a stairwell, a doorway) is its frame's origin.
#[must_use]
pub fn middle(land: &DatLandSource, cell: CellId) -> Option<Position> {
    if cell.0 & 0xFFFF >= 0x0100 {
        let g = dereth_physics::LandSource::env_cell(land, cell)?;
        let at = largest_floor(&g).unwrap_or(g.frame.origin);
        return Some(Position::new(cell, Frame::new(at, Quat::IDENTITY)));
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

/// The middle of the largest polygon of `g` a body stands on, in its landblock's space: an upper
/// room's floor, not the building's base its frame stands on.
fn largest_floor(g: &dereth_physics::source::EnvCellGeometry) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for poly in g.physics_polygons.iter() {
        let v: Vec<Vec3> = poly
            .vertices
            .iter()
            .map(|p| dereth_primitives::frame::localtoglobal(&g.frame, *p))
            .collect();
        let Some(&a) = v.first() else { continue };
        // The polygon's area and middle, a fan of triangles from its first corner.
        let (mut area, mut sum, mut up) = (0.0_f32, Vec3::new(0.0, 0.0, 0.0), 0.0_f32);
        let mut n = Vec3::new(0.0, 0.0, 0.0);
        for w in v[1..].windows(2) {
            let (e1, e2) = (
                Vec3::new(w[0].x - a.x, w[0].y - a.y, w[0].z - a.z),
                Vec3::new(w[1].x - a.x, w[1].y - a.y, w[1].z - a.z),
            );
            let c = Vec3::new(
                e1.y * e2.z - e1.z * e2.y,
                e1.z * e2.x - e1.x * e2.z,
                e1.x * e2.y - e1.y * e2.x,
            );
            n = Vec3::new(n.x + c.x, n.y + c.y, n.z + c.z);
            let t = (c.x * c.x + c.y * c.y + c.z * c.z).sqrt() / 2.0;
            area += t;
            up += c.z;
            sum = Vec3::new(
                sum.x + t * (a.x + w[0].x + w[1].x) / 3.0,
                sum.y + t * (a.y + w[0].y + w[1].y) / 3.0,
                sum.z + t * (a.z + w[0].z + w[1].z) / 3.0,
            );
        }
        let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
        if area <= 0.0 || len <= 0.0 || up / len < dereth_physics::globals::FLOOR_Z {
            continue;
        }
        if best.is_none_or(|(b, _)| area > b) {
            best = Some((area, Vec3::new(sum.x / area, sum.y / area, sum.z / area)));
        }
    }
    best.map(|(_, at)| at)
}

/// A planner over `land`, its setups and outdoor placements read from `store` as needed.
///
/// # Errors
/// The region's data does not read.
pub fn planner(
    land: &Arc<DatLandSource>,
    store: &Arc<RetailDatStore>,
    tiles: Option<&Path>,
) -> Result<Planner, String> {
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
    let nav = NavWorld::new(
        LandNavSource {
            land: Arc::clone(land),
            setups,
            land_statics: placements,
        },
        NavConfig::default(),
    );
    Ok(match tiles {
        Some(dir) => nav.with_tile_store(Box::new(CacheTiles::for_store(dir, store))),
        None => nav,
    })
}

/// Where the planner keeps the landblock tiles and dungeons it builds between sessions: a folder
/// of the cache it is given (the desktop client's is `navigation` in the player's cache folder,
/// on Windows `%LOCALAPPDATA%\Dereth\navigation`), one for each set of the data the planner reads:
/// the cell file's and the portal file's fingerprints and, where a world's data patch is laid
/// over them, what that patch holds. A tile is built from the player's own files, never shipped.
#[derive(Debug)]
pub struct CacheTiles {
    dir: PathBuf,
}

/// The most files kept in one data set's folder: past it the folder is emptied and filled again.
const CACHE_FILES: usize = 1024;

impl CacheTiles {
    /// The cache in `root` for `store`'s data as the planner reads it.
    #[must_use]
    pub fn for_store(root: &Path, store: &RetailDatStore) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |b: &[u8]| {
            for &x in b {
                h ^= u64::from(x);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for file in [store.cell(), store.portal()] {
            eat(&dereth_dat::overlay::fingerprint(file));
            // A world's patch over the file: every record it holds and every one it takes out.
            if let Some(layer) = file.layer() {
                eat(b"layer");
                for (id, e) in layer.records() {
                    for v in [id.0, e.offset, e.size, e.date, e.iteration] {
                        eat(&v.to_le_bytes());
                    }
                }
                for t in layer.tombstones() {
                    for v in [t.id, t.mask, t.iteration] {
                        eat(&v.to_le_bytes());
                    }
                }
            }
        }
        Self {
            dir: root.join(format!("{h:016x}")),
        }
    }
}

impl dereth_navigation::TileStore for CacheTiles {
    fn load(&self, key: &str) -> Option<Vec<u8>> {
        crate::platform::files::read(&self.dir.join(key)).ok()
    }

    fn save(&self, key: &str, bytes: &[u8]) {
        use crate::platform::files;
        if files::list(&self.dir).is_ok_and(|f| f.len() >= CACHE_FILES) {
            for f in files::list(&self.dir).unwrap_or_default() {
                let _ = files::remove_file(&f);
            }
        }
        // A file cut short or garbled fails its checksum when read back, and is let go of and
        // built again.
        let written =
            files::make_dirs(&self.dir).and_then(|()| files::write(&self.dir.join(key), bytes));
        if let Err(e) = written {
            tracing::debug!("a navigation tile was not kept: {e}");
        }
    }

    fn forget(&self, key: &str) {
        let _ = crate::platform::files::remove_file(&self.dir.join(key));
    }
}

/// The travel: one journey at a time, and this world's portal and recall knowledge.
pub struct Travel {
    files: StoreFiles,
    portals: PortalStore,
    /// This world's places, and what is being learned of them ([`places`]).
    places: places::Learning,
    /// What the server publishes, kept to merge into each world's store as it loads: its
    /// portal table, and its places.
    published: Vec<dereth_navigation::PortalEdge>,
    published_places: Vec<dereth_navigation::KnownPlace>,
    /// A place package taken in as text ([`Self::take_hints`]): the portals and places of the
    /// world's era, as hints.
    hinted: Vec<dereth_navigation::PortalEdge>,
    hinted_places: Vec<dereth_navigation::KnownPlace>,
    /// The dat store, as the last frame gave it: what a portal's body is read from.
    dats: Option<Arc<RetailDatStore>>,
    /// The character's body spheres, as the last frame gave them.
    body: Vec<reach::BodySphere>,
    /// Each portal body's reach for that body, by setup and scale: `None` for one whose setup does
    /// not read.
    reaches: HashMap<(u32, u32), Option<f32>>,
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
    /// Whether the walk key is held: a drop is stepped off at a walk.
    walk_held: bool,
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
            places: places::Learning::default(),
            published: Vec::new(),
            published_places: Vec::new(),
            hinted: Vec::new(),
            hinted_places: Vec::new(),
            dats: None,
            body: Vec::new(),
            reaches: HashMap::new(),
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
            walk_held: false,
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
                Leg::Drop { drop } => drop.stand,
            });
        let mut shown = t.leg;
        if let Some(j) = t.journey.as_ref() {
            route.extend(j.legs.iter().skip(t.leg).filter_map(|l| match l {
                Leg::Walk { to } => Some(*to),
                Leg::Portal { edge } => edge.to,
                Leg::Teleport { teleport } => Some(teleport.to),
                Leg::HandBack { hand_back } => Some(hand_back.to),
                Leg::Drop { drop } => Some(drop.land),
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
            match Planning::new(land, store, self.files.tiles.clone()) {
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
        self.load_places(world);
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
        // The place package of the world's era: hints, whatever its file says it knows.
        if let Some(path) = self.files.hints.as_ref().map(|d| d.join(HINT_PORTALS)) {
            match crate::platform::files::read_to_string(&path) {
                Ok(text) => match PortalStore::from_text(&text) {
                    Ok(h) => self.portals.merge(h.portals().iter().cloned().map(|mut e| {
                        e.source = EdgeSource::Hint;
                        e
                    })),
                    Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
                },
                Err(e) => tracing::warn!(target: "travel", "{}: {e}", path.display()),
            }
        }
        self.portals.merge(self.hinted.iter().cloned());
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

    /// Take in a document the server publishes, told by its first line: its portal table
    /// ([`Self::set_published`]) or its places ([`Self::set_published_places`]). How many
    /// portals or places it held.
    ///
    /// # Errors
    /// The text is neither.
    pub fn take_published(&mut self, text: &str) -> Result<usize, String> {
        if text.starts_with("dereth-places-store") {
            self.set_published_places(text)
        } else {
            self.set_published(text)
        }
    }

    /// Take in a document of a place package, told by its first line: its portal table or its
    /// places, as `dereth-places` builds them for the world's era (the web client reads them from
    /// the relay it plays through). Every edge and place is a hint, whatever the document says
    /// it knows: under what the character learns and what the server publishes, merged now and
    /// into each world's stores as they load, and never saved. How many it held.
    ///
    /// # Errors
    /// The text is neither.
    pub fn take_hints(&mut self, text: &str) -> Result<usize, String> {
        if text.starts_with("dereth-places-store") {
            let table =
                dereth_navigation::PlaceStore::from_text(text).map_err(|e| e.to_string())?;
            self.hinted_places = table
                .places()
                .iter()
                .cloned()
                .map(|mut p| {
                    p.source = EdgeSource::Hint;
                    p
                })
                .collect();
            self.merge_hinted_places();
            tracing::info!(
                target: "travel",
                "the era's places: {} as hints",
                self.hinted_places.len()
            );
            return Ok(self.hinted_places.len());
        }
        let table = PortalStore::from_text(text).map_err(|e| e.to_string())?;
        self.hinted = table
            .portals()
            .iter()
            .cloned()
            .map(|mut e| {
                e.source = EdgeSource::Hint;
                e
            })
            .collect();
        self.portals.merge(self.hinted.iter().cloned());
        tracing::info!(
            target: "travel",
            "the era's portals: {} as hints",
            self.hinted.len()
        );
        Ok(self.hinted.len())
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
        // The door whose lever the walk is going to: shut to the way there, which never goes
        // through it.
        let to_its_lever = match self.trip.as_ref().and_then(|t| t.step.as_ref()) {
            Some(Step::Lever { door, .. }) => Some(door.0),
            _ => None,
        };
        let mut kept = Vec::with_capacity(list.len());
        self.creatures_seen.clear();
        let mut seen = BTreeMap::new();
        for mut o in list {
            if locked.contains(&o.obstacle.id.0)
                || (to_its_lever == Some(o.obstacle.id.0)
                    && o.obstacle.kind == ObstacleKind::Door(DoorState::Closed))
            {
                o.obstacle.kind = ObstacleKind::Door(DoorState::Locked);
            } else if o.obstacle.kind == ObstacleKind::Door(DoorState::Locked)
                && to_its_lever != Some(o.obstacle.id.0)
                && !levers_of(self.places.store(), &o.obstacle.position).is_empty()
            {
                // A door no use opens, which a lever does: the walk goes through by the lever.
                o.obstacle.kind = ObstacleKind::Door(DoorState::Closed);
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

    /// The objects and creatures within `reach` across of `p` and a body's height up or down,
    /// nearest first, for the log: one on the floor above or below is not in the way.
    fn near(&self, p: &Position, reach: f32) -> Vec<String> {
        const RISE: f32 = 2.0;
        let here = global(p);
        let creatures = self.creatures_seen.iter().filter_map(|(name, q)| {
            let d = hypotf(q.x - here.x, q.y - here.y);
            (d <= reach && (q.z - here.z).abs() <= RISE)
                .then(|| (d, format!("creature {name} at {d:.1} m")))
        });
        let mut v: Vec<(f32, String)> = self
            .obstacles
            .iter()
            .filter(|o| o.obstacle.kind != ObstacleKind::Creature)
            .filter_map(|o| {
                let q = global(&o.obstacle.position);
                let d = hypotf(q.x - here.x, q.y - here.y);
                (d <= reach && (q.z - here.z).abs() <= RISE).then(|| {
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
        if self.walk_held {
            out.push(TravelCommand::Walk(false));
            self.walk_held = false;
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
                    door: t.shut_door.filter(|_| reason.starts_with(DOOR_SHUT)),
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
        let unwalkable = self
            .trip
            .as_ref()
            .map(|t| t.unwalkable.clone())
            .unwrap_or_default();
        let walkable = |e: &&dereth_navigation::PortalEdge| {
            !unwalkable
                .iter()
                .any(|u| dereth_navigation::global_distance(u, &e.from) < UNWALKABLE_NEAR)
        };
        let (portals, refused, hand_backs) = if allow.portals {
            let traveller = dereth_navigation::Traveller::at_level(self.character.level);
            let name = self.character.name.as_str();
            let now = unix_now();
            let mine: Vec<dereth_navigation::PortalEdge> = self
                .portals
                .routable_for(&traveller)
                .into_iter()
                .filter(walkable)
                .filter(|e| self.portals.refused_for(name, e, now).is_none())
                .cloned()
                .collect();
            // The portals anyone may take that this character may not, each with why: with no
            // journey, they say what stands in the way.
            let refused: Vec<(dereth_navigation::PortalEdge, dereth_navigation::Refusal)> = self
                .portals
                .routable_for_anyone()
                .into_iter()
                .filter(walkable)
                .filter_map(|e| {
                    e.refusal(&traveller)
                        .or_else(|| {
                            self.portals
                                .refused_for(name, e, now)
                                .map(|why| dereth_navigation::Refusal::Retired(why.to_owned()))
                        })
                        .map(|r| (e.clone(), r))
                })
                .collect();
            (mine, refused, self.hand_backs())
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        self.planning.as_ref()?;
        let id = self.job();
        self.planning.as_mut()?.ask(Job::Journey {
            id,
            from: *from,
            to,
            portals,
            refused,
            teleports,
            hand_backs,
            reach: object.then_some(OBJECT_REACH),
            // Only a drop the character lands from unhurt; none with its jump unknown.
            highest_drop: self
                .character
                .jump_speed
                .map_or(0.0, dereth_navigation::survivable_drop),
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
             jump {:?} m/s, drops to {:.1} m; allowed {allow:?}; stance {:?}",
            c.name,
            c.recall_spells,
            c.recall_items,
            c.recall_commands.len(),
            c.can_cast,
            c.mana,
            c.jump_speed,
            c.jump_speed.map_or(0.0, dereth_navigation::survivable_drop),
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
            // Planned again without a portal no walk reaches: that is why there is no way.
            let why = reason
                .as_deref()
                .filter(|r| r.starts_with(UNWALKABLE))
                .unwrap_or("no_path")
                .to_owned();
            self.end(out, Some(here), Some(&why));
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
        let (mut ask, mut refused) = (None, None);
        if let Some(t) = self.trip.as_mut() {
            t.stats.journeys += 1;
            t.journey = Some(journey);
            t.view = view;
            t.leg = 0;
            // Handing a token back is for good: the player says yes first, once a trip; or
            // whoever asked for the journey agreed already; or nobody can, and it goes no further.
            if let Some(h) = hand_back.filter(|_| !t.confirmed) {
                match t.allow.hand_backs {
                    HandBacks::Ask => {
                        t.step = Some(Step::Ask);
                        ask = Some(h);
                    }
                    HandBacks::Agreed => t.confirmed = true,
                    HandBacks::Refused => refused = Some(h),
                }
            }
        }
        if let Some(h) = refused {
            let why = self
                .hand_backs
                .iter()
                .find(|s| Some(s.object) == h.object || s.name == h.name)
                .map_or_else(
                    || {
                        format!(
                            "needs the player to agree to hand the {} back to {}",
                            h.token, h.name
                        )
                    },
                    |s| s.refused.clone(),
                );
            tracing::info!(target: "travel", "the journey hands a token back unagreed: {why}");
            self.end(out, Some(here), Some(&why));
            return;
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
            Leg::Drop { drop } => {
                // Facing where it lands, walk at the ledge and hop off it with the least jump (the
                // edge holds a walking body back): at a walk, so it comes down near there rather
                // than carried on past it.
                self.release_keys(out);
                if let Some(t) = self.trip.as_mut() {
                    t.follower = None;
                }
                let (a, b) = (global(here), global(&drop.land));
                let heading = dereth_primitives::num::math::atan2f(b.x - a.x, b.y - a.y)
                    .to_degrees()
                    .rem_euclid(360.0);
                tracing::info!(
                    target: "travel",
                    "stepping off the ledge at {:08X} {:?}, {:.1} m down to {:08X}",
                    drop.stand.cell.0,
                    drop.stand.frame.origin,
                    drop.height,
                    drop.land.cell.0
                );
                out.push(TravelCommand::Face(heading));
                out.push(TravelCommand::Walk(true));
                out.push(TravelCommand::Forward(true));
                self.walk_held = true;
                self.forward = true;
                self.set_step(Step::Drop {
                    drop,
                    since: self.wall,
                    hops: 0,
                    off: false,
                    last_z: a.z,
                    still: 0,
                });
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

    /// A step at a ledge: walking off it, then falling. Once the body has stopped coming down,
    /// near where the drop lands the journey goes on; landed elsewhere, it is planned again
    /// from there. A ledge it cannot step off, or a fall that does not end, ends the journey.
    fn drop_frame(&mut self, p: &Position, out: &mut Vec<TravelCommand>) {
        /// Seconds walking at the ledge before the first hop, between hops, the most hops, and
        /// seconds to come down after.
        const FIRST_HOP: f64 = 0.3;
        const NEXT_HOP: f64 = 1.5;
        const HOPS: u32 = 3;
        const FALL: f64 = 10.0;
        let Some(Step::Drop {
            drop,
            since,
            mut hops,
            mut off,
            mut last_z,
            mut still,
        }) = self.trip.as_ref().and_then(|t| t.step.clone())
        else {
            return;
        };
        let (here, stand, land) = (global(p), global(&drop.stand), global(&drop.land));
        let step_off = FIRST_HOP + f64::from(HOPS) * NEXT_HOP;
        if !off {
            if stand.z - here.z > 1.0 {
                // Off the ledge: its keys go, and the fall is the world's.
                self.release_keys(out);
                off = true;
            } else if hops < HOPS && self.wall - since >= FIRST_HOP + f64::from(hops) * NEXT_HOP {
                out.push(TravelCommand::Jump);
                hops += 1;
            } else if self.wall - since > step_off {
                self.end(
                    out,
                    Some(p),
                    Some(&format!(
                        "drop_blocked: the ledge at {:08X} did not let the body step off",
                        drop.stand.cell.0
                    )),
                );
                return;
            }
        } else {
            still = if (here.z - last_z).abs() < 0.02 {
                still + 1
            } else {
                0
            };
            if still >= 3 {
                let across = hypotf(here.x - land.x, here.y - land.y);
                tracing::info!(
                    target: "travel",
                    "came down at {:08X} {:?}, {across:.1} m across from where the drop lands",
                    p.cell.0,
                    p.frame.origin
                );
                if across <= 6.0 && (here.z - land.z).abs() <= 1.5 {
                    if let Some(t) = self.trip.as_mut() {
                        t.step = None;
                        t.leg += 1;
                    }
                    self.start_leg(p, out);
                } else {
                    if let Some(t) = self.trip.as_mut() {
                        t.step = None;
                        t.stats.replans += 1;
                    }
                    self.carry_on(p, out, Some(String::from("came down elsewhere")));
                }
                return;
            }
            if self.wall - since > step_off + FALL {
                self.end(
                    out,
                    Some(p),
                    Some("drop_failed: the body did not come down"),
                );
                return;
            }
        }
        last_z = here.z;
        if let Some(t) = self.trip.as_mut() {
            t.step = Some(Step::Drop {
                drop,
                since,
                hops,
                off,
                last_z,
                still,
            });
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

    /// The object standing where the lever at `lever` stands (not the door it works); `None` for
    /// one that is no object in sight, or is stepped on rather than used (a pressure plate).
    fn lever_object(&self, lever: &Position, door: ObjectId) -> Option<ObjectId> {
        self.obstacles
            .iter()
            .filter(|o| o.obstacle.id != door && o.obstacle.kind != ObstacleKind::Creature)
            .map(|o| (global_distance(&o.obstacle.position, lever), o.obstacle.id))
            .filter(|(d, _)| *d < LEVER_NEAR)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    /// At a door no use opens whose levers are known: off to the nearest of them, unless the
    /// walk has gone to them [`DOOR_TRIES`] times already, when the door is planned round.
    fn go_to_levers(
        &mut self,
        here: &Position,
        door: ObjectId,
        mut levers: Vec<Position>,
        out: &mut Vec<TravelCommand>,
    ) {
        levers.sort_by(|a, b| global_distance(here, a).total_cmp(&global_distance(here, b)));
        let Some(t) = self.trip.as_mut() else { return };
        let goes = t.lever_goes.entry(door.0).or_insert(0);
        if *goes >= DOOR_TRIES {
            self.door_stays_shut(here, door, DoorWhy::LeverFailed, out);
            return;
        }
        *goes += 1;
        // Back at the door after its lever, the walk goes on with what it was doing.
        let resume = match t.step.take() {
            Some(Step::Lever { resume, .. }) => resume,
            Some(step) => Box::new(step),
            None => Box::new(Step::Walk {
                target: *here,
                last: false,
            }),
        };
        let first = levers[0];
        tracing::info!(
            target: "travel",
            "door {:08X} opens by a lever: to the one at {:08X} {:?}",
            door.0,
            first.cell.0,
            first.frame.origin
        );
        t.step = Some(Step::Lever {
            door,
            levers,
            which: 0,
            used: None,
            resume,
        });
        // The way to a lever never goes through its door: a lever beyond it is no way to open it.
        let list = std::mem::take(&mut self.obstacles);
        self.set_obstacles(list, Some(here));
        self.walk_toward(here, &first, out);
    }

    /// At the lever being walked to: once the door stands open, on with the walk; else work the
    /// lever (use it, or stand on a plate), again after a while, and after [`DOOR_TRIES`] tries
    /// plan round the door.
    fn at_lever(&mut self, here: &Position, out: &mut Vec<TravelCommand>) {
        let Some(Step::Lever {
            door,
            levers,
            which,
            used,
            resume,
        }) = self.trip.as_ref().and_then(|t| t.step.clone())
        else {
            return;
        };
        let open = self.obstacles.iter().any(|o| {
            o.obstacle.id == door && o.obstacle.kind == ObstacleKind::Door(DoorState::Open)
        });
        if open {
            tracing::info!(target: "travel", "door {:08X} is open: back to it", door.0);
            self.set_step(*resume);
            let list = std::mem::take(&mut self.obstacles);
            self.set_obstacles(list, Some(here));
            self.replan_leg(here, out);
            return;
        }
        match used {
            Some((since, _)) if self.wall - since <= DOOR_WAIT => {}
            Some((_, n)) if n >= DOOR_TRIES => {
                self.door_stays_shut(here, door, DoorWhy::LeverFailed, out);
            }
            _ => {
                let n = used.map_or(0, |(_, n)| n) + 1;
                let lever = self.lever_object(&levers[which], door);
                tracing::info!(
                    target: "travel",
                    "working the lever {} for door {:08X} ({n} of {DOOR_TRIES})",
                    lever.map_or_else(|| String::from("underfoot"), |l| format!("{:08X}", l.0)),
                    door.0
                );
                self.release_keys(out);
                if let Some(lever) = lever {
                    out.push(TravelCommand::Use(lever));
                }
                self.set_step(Step::Lever {
                    door,
                    levers,
                    which,
                    used: Some((self.wall, n)),
                    resume,
                });
            }
        }
    }

    /// No walk reaches the lever being walked to: on to the next, or with none left, the door
    /// is planned round.
    fn next_lever(&mut self, here: &Position, out: &mut Vec<TravelCommand>) {
        let Some(Step::Lever {
            door,
            levers,
            which,
            resume,
            ..
        }) = self.trip.as_ref().and_then(|t| t.step.clone())
        else {
            return;
        };
        if which + 1 >= levers.len() {
            self.door_stays_shut(here, door, DoorWhy::NoLever, out);
            return;
        }
        let next = levers[which + 1];
        tracing::info!(
            target: "travel",
            "no walk reaches that lever: to the one at {:08X} {:?}",
            next.cell.0,
            next.frame.origin
        );
        self.set_step(Step::Lever {
            door,
            levers,
            which: which + 1,
            used: None,
            resume,
        });
        self.walk_toward(here, &next, out);
    }

    /// A door that stays shut, for `why`: planned round from now on, the walk on with what it
    /// was doing.
    fn door_stays_shut(
        &mut self,
        here: &Position,
        door: ObjectId,
        why: DoorWhy,
        out: &mut Vec<TravelCommand>,
    ) {
        let said = match why {
            DoorWhy::NoLever => "has no lever a walk reaches",
            _ => "did not open from its lever",
        };
        let name = self
            .obstacles
            .iter()
            .find(|o| o.obstacle.id == door)
            .map_or_else(|| format!("door {:08X}", door.0), |o| o.name.clone());
        tracing::warn!(target: "travel", "door {:08X} {said}; planning round it", door.0);
        let Some(t) = self.trip.as_mut() else { return };
        if let Some(Step::Lever { resume, .. }) = t.step.take() {
            t.step = Some(*resume);
        }
        t.shut = Some(format!("{name} {said}"));
        t.shut_door = Some(ShutDoor { object: door, why });
        t.door = None;
        t.locked.insert(door.0);
        t.route_dirty = None;
        t.stats.replans += 1;
        let list = std::mem::take(&mut self.obstacles);
        self.set_obstacles(list, Some(here));
        self.replan_leg(here, out);
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

    /// What the client knows of this world's portals, by where it learned them: `none known`, or
    /// `12 learned, 4,269 from the era's package`, as a failed journey says it.
    fn portals_known(&self) -> String {
        let (mut learned, mut published, mut hints) = (0usize, 0usize, 0usize);
        for e in self
            .portals
            .portals()
            .iter()
            .filter(|e| e.retired.is_none())
        {
            match e.source {
                EdgeSource::Learned => learned += 1,
                EdgeSource::Published => published += 1,
                EdgeSource::Hint => hints += 1,
            }
        }
        let parts: Vec<String> = [
            (learned, "learned"),
            (published, "published by the server"),
            (hints, "from the era's package"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if parts.is_empty() {
            String::from(
                "no portals known: the server publishes none, and no era package is loaded",
            )
        } else {
            format!("portals known: {}", parts.join(", "))
        }
    }

    /// Where the portal stands that a route ending at `end` would walk onto without taking it,
    /// and how far from it the route must end: the nearest portal whose keeping-off distance
    /// ([`Self::keep_off`]) `end` is within, when the journey's walk there is not to use it.
    fn portal_kept_off(&mut self, end: &Position) -> Option<(Position, f32)> {
        if self.portal_walked_to(end).is_some() {
            return None;
        }
        let near: Vec<(Position, Option<u32>, Option<PortalShape>)> = self
            .portals
            .portals()
            .iter()
            .filter(|e| e.retired.is_none())
            .filter(|e| dereth_navigation::global_distance(&e.from, end) < PORTAL_LOOK)
            .map(|e| (e.from, e.wcid, e.shape))
            .collect();
        let mut found: Option<(f32, Position, f32)> = None;
        for (from, wcid, shape) in &near {
            // What one edge of a portal knows of its body, another may not.
            let shape = shape.or_else(|| {
                near.iter()
                    .filter(|(f, w, _)| {
                        w == wcid && dereth_navigation::global_distance(f, from) < 3.0
                    })
                    .find_map(|(_, _, s)| *s)
            });
            let Some(keep) = self.keep_off(shape) else {
                continue;
            };
            let d = dereth_navigation::global_distance(from, end);
            if d < keep && found.is_none_or(|(best, _, _)| d < best) {
                found = Some((d, *from, keep));
            }
        }
        found.map(|(_, from, keep)| (from, keep))
    }

    /// How far from a portal of body `shape` a walk that is not taking it ends: its reach for the
    /// character's body and [`PORTAL_MARGIN`]; [`PORTAL_KEEP_OFF`] when its body is not known or
    /// does not read; `None` for a portal a body is never taken by touching.
    fn keep_off(&mut self, shape: Option<PortalShape>) -> Option<f32> {
        let Some(shape) = shape else {
            return Some(PORTAL_KEEP_OFF);
        };
        if !shape.touches {
            return None;
        }
        let key = (shape.setup, shape.scale.to_bits());
        let reach = match self.reaches.get(&key) {
            Some(r) => *r,
            None => {
                let body: &[reach::BodySphere] = if self.body.is_empty() {
                    &reach::HUMAN_BODY
                } else {
                    &self.body
                };
                let measured = self.dats.as_ref().and_then(|store| {
                    let geometry = dereth_world_data::env_cells::static_geometry(
                        store,
                        DataId(shape.setup),
                        &mut dereth_world_data::setup::SetupPartStats::default(),
                    )?;
                    Some(reach::reach(&geometry, shape.scale, body))
                });
                self.reaches.insert(key, measured);
                measured
            }
        };
        Some(reach.map_or(PORTAL_KEEP_OFF, |r| r + PORTAL_MARGIN))
    }

    /// The name of the portal the trip's journey walks to `to` to use, when the walk to `to` is
    /// one that ends at a portal.
    fn portal_walked_to(&self, to: &Position) -> Option<String> {
        let t = self.trip.as_ref()?;
        let legs = &t.journey.as_ref()?.legs;
        legs.windows(2).find_map(|w| match (&w[0], &w[1]) {
            (Leg::Walk { to: walk }, Leg::Portal { edge })
                if dereth_navigation::global_distance(walk, to) < UNWALKABLE_NEAR
                    && dereth_navigation::global_distance(&edge.from, to) < UNWALKABLE_NEAR =>
            {
                Some(edge.name.clone())
            }
            _ => None,
        })
    }

    /// Ask for the route as far toward `to` as one window holds; the body stands until it comes,
    /// and then follows it.
    fn walk_toward(&mut self, here: &Position, to: &Position, out: &mut Vec<TravelCommand>) {
        // The walk to the object a journey goes to ends within reach of it.
        let reach = self
            .trip
            .as_ref()
            .filter(|t| matches!(t.target, TravelTarget::Object(_)) && t.goal == Some(*to))
            .map(|_| OBJECT_REACH)
            .or_else(|| match self.trip.as_ref().and_then(|t| t.step.as_ref()) {
                // A lever is used from within reach; a plate is stood on.
                Some(Step::Lever { door, .. }) => {
                    self.lever_object(to, *door).map(|_| OBJECT_REACH)
                }
                _ => None,
            });
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
                if matches!(
                    self.trip.as_ref().and_then(|t| t.step.as_ref()),
                    Some(Step::Lever { .. })
                ) {
                    self.next_lever(here, out);
                    return;
                }
                // Only a drop leads on: the walk's places are built now, and its floors no walk
                // joins are told apart, so the journey is planned again once, to find a portal
                // round or, with nothing else, the drop.
                if matches!(e, NoPath::OnlyByDrop { .. }) {
                    if let Some(t) = self.trip.as_mut().filter(|t| !t.drop_replanned) {
                        t.drop_replanned = true;
                        t.stats.replans += 1;
                        self.plan_trip(here, out, Some(format!("only a drop leads on: {e}")));
                        return;
                    }
                }
                // A walk to a portal that the land does not join to where the body is: the
                // journey took the land for one place. It is planned again without that portal.
                if e == NoPath::Unreachable {
                    if let Some(name) = self.portal_walked_to(&to) {
                        let Some(t) = self.trip.as_mut() else { return };
                        if t.unwalkable.len() < MAX_UNWALKABLE {
                            t.unwalkable.push(to);
                            t.stats.replans += 1;
                            tracing::info!(
                                target: "travel",
                                "no walk reaches the portal {name:?} from here: planning without it"
                            );
                            self.plan_trip(here, out, Some(format!("{UNWALKABLE} {name}")));
                            return;
                        }
                    }
                }
                // A journey that is all walking, its walk finding no way: no portal known led
                // there, and what the client knows says why.
                let on_foot = self
                    .trip
                    .as_ref()
                    .and_then(|t| t.journey.as_ref())
                    .is_some_and(|j| j.legs.iter().all(|l| matches!(l, Leg::Walk { .. })));
                let known = self.portals_known();
                let Some(t) = self.trip.as_mut() else { return };
                t.follower = None;
                t.stats.replans += 1;
                // Where only a drop leads on, the journey names it: a jump there is the player's
                // to make. A door on the way that would not open is named next, as what stands in
                // the way.
                let why = match e {
                    NoPath::OnlyByDrop { .. } => format!("only_by_drop: {e}"),
                    _ if !t.locked.is_empty() => t
                        .shut
                        .as_ref()
                        .map_or_else(|| String::from(DOOR_SHUT), |w| format!("{DOOR_SHUT}: {w}")),
                    _ if on_foot => format!(
                        "no_route: no walk was found there, and no portal known leads there \
                         ({known})"
                    ),
                    _ => String::from("no_route"),
                };
                if t.stats.replans > MAX_REPLANS {
                    self.end(out, Some(here), Some(&why));
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
                // A door that did not open may only have been slow, or had something standing in
                // it: with no way round, try those doors once more. One the server said is locked,
                // and one that has had its second go, stays planned round.
                let again: BTreeSet<u32> = t
                    .locked
                    .iter()
                    .filter(|d| !t.lock_answers.contains(d) && !t.tried_again.contains(d))
                    .copied()
                    .collect();
                if again.is_empty() {
                    self.end(out, Some(here), Some(&why));
                    return;
                }
                tracing::info!(
                    target: "travel",
                    "trying the {} doors that did not open again",
                    again.len()
                );
                t.locked.retain(|d| !again.contains(d));
                t.tried_again.extend(&again);
                let mut list = std::mem::take(&mut self.obstacles);
                for o in &mut list {
                    if o.obstacle.kind == ObstacleKind::Door(DoorState::Locked)
                        && again.contains(&o.obstacle.id.0)
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
    fn follow_route(&mut self, mut path: NavPath, last: bool) {
        // A route that ends on a portal the journey is not taking ends short of it: a body that
        // walks into a portal is carried off.
        let end = path.waypoints.last().map(|w| w.position);
        if let Some((portal, keep)) = end.and_then(|end| self.portal_kept_off(&end)) {
            if path.stop_short_of(&portal, keep) {
                tracing::info!(
                    target: "travel",
                    "the route ends {keep:.2} m short of a portal it is not taking, at {:#010X} \
                     {:.2} {:.2}",
                    portal.cell.0,
                    portal.frame.origin.x,
                    portal.frame.origin.y
                );
            }
        }
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
                to: None,
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

    /// The planning's answers this frame, taken up with the body at `here`. With no body there
    /// is nothing to take them up with: they wait for a frame that has one, so a trip waiting
    /// on its plan never loses it.
    fn answers(&mut self, here: Option<&Position>, out: &mut Vec<TravelCommand>) {
        if here.is_none() {
            return;
        }
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
                    why,
                } => {
                    let first = first.and_then(Result::ok).map(|(p, _)| p);
                    if let Some(request) = self.plans_asked.remove(&id) {
                        let plan = match (to, journey) {
                            (None, _) => Err(String::from("no_target")),
                            (_, None) => Err(why.unwrap_or_else(|| String::from("no_path"))),
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
                        // No way for this character, and the portals that would lead there say
                        // why: that is the journey's end.
                        if journey.is_none() && to.is_some() {
                            if let Some(why) = why {
                                self.end(out, Some(p), Some(&why));
                                continue;
                            }
                        }
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
        for seen in &input.portals {
            self.portals.note_shape(&seen.sighting);
        }
        if self.dats.is_none() {
            self.dats = Some(Arc::clone(input.store));
        }
        if !input.body.is_empty() && input.body != self.body {
            self.body.clone_from(&input.body);
            self.reaches.clear();
        }
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
        if self.notice(&p, input.portal_refused, &mut out) {
            return out;
        }
        let Some(t) = self.trip.as_ref() else {
            return out;
        };
        let (begun, purpose, target) =
            (t.journey.is_none() && t.step.is_none(), t.purpose, t.target);
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

    /// What the body did since the last step, before the journey's frame: a portal's refusal the
    /// server sent (`refused`), the ground walked, and a teleport, by a jump of a teleport's
    /// length or a set-down where a portal the body stood beside sends. Answers whether the step
    /// ends here: a refusal dealt with, or no journey.
    fn notice(&mut self, p: &Position, refused: Option<u32>, out: &mut Vec<TravelCommand>) -> bool {
        if let Some(code) = refused {
            if self.portal_refused(p, code, out) {
                return true;
            }
        }
        // Set down where a portal the body stood beside sends: a teleport, however short.
        let set_down = self
            .trip
            .as_ref()
            .and_then(|t| t.last)
            .filter(|l| global_distance(l, p) > LANDS_WITHIN)
            .and_then(|l| self.portal_set_down(&l, p));
        let Some(t) = self.trip.as_mut() else {
            return true;
        };
        let mut jumped = None;
        if let Some(l) = t.last {
            let (a, b) = (global(p), global(&l));
            let d = hypotf(a.x - b.x, a.y - b.y);
            // A teleport is no walk.
            if d < 30.0 && set_down.is_none() {
                t.stats.walked += d;
            } else {
                jumped = Some(l);
            }
        }
        t.last = Some(*p);
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
                    shape: None,
                });
            taken = Some((
                match (&t.step, t.journey.as_ref().and_then(|j| j.legs.get(t.leg))) {
                    (Some(Step::Approach { .. }), Some(Leg::Portal { edge })) => {
                        Awaited::Portal(PortalSighting {
                            wcid: edge.wcid,
                            name: edge.name.clone(),
                            position: edge.from,
                            shape: None,
                        })
                    }
                    _ => set_down
                        .clone()
                        .or(known)
                        .map_or(Awaited::Elsewhere, Awaited::WalkedInto),
                },
                before,
            ));
            t.follower = None;
        }
        if let Some((what, before)) = taken {
            self.release_keys(out);
            // Set down where the portal sends: landed already, however short the way.
            let mut wait = self.wait(what, &before);
            if let (Some(_), Step::Wait { watch, .. }) = (&set_down, &mut wait) {
                watch.to = Some(*p);
            }
            self.set_step(wait);
        }
        false
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
            Step::Drop { .. } => {
                self.drop_frame(p, out);
                return;
            }
            Step::Approach { portal } if global_distance(p, &portal) < PORTAL_REACH => {
                self.use_portal(p, &portal, out);
                return;
            }
            // At the lever: within reach of one to use, or on a plate.
            Step::Lever {
                door,
                ref levers,
                which,
                used,
                ..
            } if used.is_some()
                || global_distance(p, &levers[which])
                    < match self.lever_object(&levers[which], door) {
                        Some(_) => OBJECT_REACH + 0.5,
                        None => LEVER_NEAR / 2.0,
                    } =>
            {
                self.at_lever(p, out);
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
                // Set down somewhere new: a portal no walk reached from where the body was may be
                // a walk away from here.
                if let Some(t) = self.trip.as_mut() {
                    t.unwalkable.clear();
                }
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
                        self.portals.forget_refusal_for(&name, s);
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
                        self.portals.forget_refusal_for(&name, s);
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

    /// The portal the body stood beside at `before` that sends where it is now (`at`): one the
    /// store knows, or the journey's next. A walk-in portal can set the body down a short way
    /// off, where no jump of a teleport's length is seen.
    fn portal_set_down(&self, before: &Position, at: &Position) -> Option<PortalSighting> {
        let journey = self
            .trip
            .as_ref()
            .and_then(|t| t.journey.as_ref().map(|j| (j, t.leg)))
            .into_iter()
            .flat_map(|(j, leg)| j.legs.iter().skip(leg).take(2))
            .filter_map(|l| match l {
                Leg::Portal { edge } => Some(edge),
                _ => None,
            });
        journey
            .chain(self.portals.portals())
            .find(|e| {
                global_distance(&e.from, before) < PORTAL_REACH + 3.0
                    && e.to
                        .is_some_and(|to| global_distance(&to, at) < LANDS_WITHIN)
            })
            .map(|e| PortalSighting {
                wcid: e.wcid,
                name: e.name.clone(),
                position: e.from,
                shape: None,
            })
    }

    /// The server refused a portal with `code` while the body used it or walked into it: the
    /// journey does without it, saying why, and a refusal that lasts is kept for the character so
    /// later journeys do too. A passing one ends the journey. Answers whether it was such a
    /// refusal, for a portal the journey was taking.
    fn portal_refused(&mut self, p: &Position, code: u32, out: &mut Vec<TravelCommand>) -> bool {
        let Some((reason, lasting)) = portal_refusal(code) else {
            return false;
        };
        let Some(t) = self.trip.as_ref() else {
            return false;
        };
        let by = |e: &dereth_navigation::PortalEdge| {
            (global_distance(&e.from, p) < PORTAL_REACH + 3.0).then(|| PortalSighting {
                wcid: e.wcid,
                name: e.name.clone(),
                position: e.from,
                shape: None,
            })
        };
        let portal = match &t.step {
            Some(Step::Wait {
                what: Awaited::Portal(s) | Awaited::WalkedInto(s),
                ..
            }) => Some(s.clone()),
            Some(Step::Walk { .. } | Step::Approach { .. }) => t
                .journey
                .as_ref()
                .into_iter()
                .flat_map(|j| j.legs.iter().skip(t.leg).take(2))
                .find_map(|l| match l {
                    Leg::Portal { edge } => by(edge),
                    _ => None,
                }),
            _ => None,
        };
        let Some(portal) = portal else {
            return false;
        };
        let id = t.id;
        tracing::warn!(
            target: "travel",
            "portal {:?} refused the character: {reason}",
            portal.name
        );
        if lasting {
            let name = self.character.name.clone();
            self.portals
                .learn_refusal_for(&name, &portal, &reason, unix_now());
            self.save_store();
        }
        self.events.push(TravelEvent::Learned {
            id,
            what: Learned::Refusal {
                name: portal.name.clone(),
                reason: reason.clone(),
            },
        });
        self.release_keys(out);
        let Some(t) = self.trip.as_mut() else {
            return true;
        };
        t.stats.refusals += 1;
        t.stats.replans += 1;
        t.step = None;
        t.follower = None;
        if !lasting {
            self.end(out, Some(p), Some(&reason));
        } else if t.stats.replans > MAX_REPLANS {
            self.end(out, Some(p), Some("too_many_replans"));
        } else {
            self.carry_on(
                p,
                out,
                Some(format!("portal {} refused: {reason}", portal.name)),
            );
        }
        true
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
                shape: None,
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
                    Some(Step::Lever { .. }) => self.at_lever(p, out),
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
                // A door with levers is opened by one of them, not by a use of the door.
                let levers = self
                    .obstacles
                    .iter()
                    .find(|o| o.obstacle.id == door)
                    .map(|o| levers_of(self.places.store(), &o.obstacle.position))
                    .filter(|l| !l.is_empty());
                if let (Some(levers), false, false) = (levers, open, answered) {
                    t.door = None;
                    self.go_to_levers(p, door, levers, out);
                    return;
                }
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
                        let name = self
                            .obstacles
                            .iter()
                            .find(|o| o.obstacle.id == door)
                            .map_or_else(|| format!("door {:08X}", door.0), |o| o.name.clone());
                        t.shut_door = Some(ShutDoor {
                            object: door,
                            why: if answered {
                                DoorWhy::Locked
                            } else {
                                DoorWhy::NoAnswer
                            },
                        });
                        t.shut = Some(if answered {
                            format!("{name} is locked, the server says")
                        } else {
                            format!("{name} did not open after {DOOR_TRIES} uses")
                        });
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
                forward: go,
                turn,
                face,
                ..
            } => {
                if turn != self.turning {
                    out.push(TravelCommand::Turn(turn));
                    self.turning = turn;
                }
                if let Some(heading) = face {
                    out.push(TravelCommand::Face(heading));
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
            shut: None,
            shut_door: None,
            lock_answers: BTreeSet::new(),
            tried_again: BTreeSet::new(),
            lever_goes: BTreeMap::new(),
            drop_replanned: false,
            disabled: BTreeSet::new(),
            unwalkable: Vec::new(),
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
            Leg::Drop { drop } => format!(
                "drop {:.1} m from {:08X} to {:08X}",
                drop.height, drop.stand.cell.0, drop.land.cell.0
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

    /// Behaviour: plugins.nav.handing-a-token-back-asks-the-player-first
    /// A journey out of the Training Academy hands Jonathan's token back. The player's own asks
    /// them; one whose asker agreed already goes on unasked; one nobody can agree to (a plugin's
    /// that did not) ends at once, saying the player must agree, rather than waiting on a
    /// question no one will answer.
    #[test]
    fn a_journey_that_hands_a_token_back_asks_goes_on_agreed_or_ends_at_once_unagreed() {
        let place = |cell: u32, x: f32, y: f32| {
            Position::new(
                CellId(cell),
                Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
            )
        };
        let here = place(0x8602_01AD, 12.3, -28.5);
        let corpse = place(0x019E_0102, 0.5, -20.2);
        let jonathan = dereth_navigation::HandBack {
            name: String::from("Jonathan"),
            object: Some(ObjectId(0x8000_0116)),
            at: place(0x8602_01B0, 22.1, -19.1),
            token: String::from("Academy Exit Token"),
            to: place(0xA9B4_0019, 84.0, 7.1),
        };
        let journey = || Journey {
            legs: vec![
                Leg::Walk { to: jonathan.at },
                Leg::HandBack {
                    hand_back: jonathan.clone(),
                },
                Leg::Walk { to: corpse },
            ],
            seconds: 130.0,
        };
        let seen = HandBackSeen {
            object: ObjectId(0x8000_0116),
            name: String::from("Jonathan"),
            at: jonathan.at,
            token: String::from("Academy Exit Token"),
            question: String::from("Leave the Training Academy?"),
            refused: String::from("needs the player to agree to leave the Training Academy"),
        };
        let plan = |hand_backs: HandBacks, in_sight: bool| {
            let mut travel = Travel::new(StoreFiles::default());
            travel.planning = Some(Planning::held());
            if in_sight {
                travel.hand_backs = vec![seen.clone()];
            }
            let allow = TravelAllow {
                hand_backs,
                ..TravelAllow::default()
            };
            let mut trip = Trip::new(1, Purpose::Go, TravelTarget::Place(corpse), allow, 0.0, 0.0);
            trip.goal = Some(corpse);
            travel.trip = Some(trip);
            let mut out = Vec::new();
            travel.planned(
                &here,
                Some(corpse),
                Some((journey(), 25.0)),
                None,
                None,
                &mut out,
            );
            (travel, out)
        };
        let failed = |travel: &Travel| {
            travel.events.iter().find_map(|e| match e {
                TravelEvent::Failed { reason, .. } => Some(reason.clone()),
                _ => None,
            })
        };
        // Nobody can agree: it ends at once, and says why.
        let (travel, out) = plan(HandBacks::Refused, true);
        assert!(travel.trip.is_none(), "it ends at once");
        assert_eq!(
            failed(&travel).as_deref(),
            Some("needs the player to agree to leave the Training Academy")
        );
        assert!(
            !out.iter().any(|c| matches!(c, TravelCommand::Ask(_))),
            "nobody is asked"
        );
        let (travel, _) = plan(HandBacks::Refused, false);
        assert_eq!(
            failed(&travel).as_deref(),
            Some("needs the player to agree to hand the Academy Exit Token back to Jonathan"),
            "the person not in sight"
        );
        // Agreed already: it goes on, asking no one.
        let (travel, out) = plan(HandBacks::Agreed, true);
        let t = travel.trip.as_ref().expect("going on");
        assert!(t.confirmed && !matches!(t.step, Some(Step::Ask)));
        assert!(!out.iter().any(|c| matches!(c, TravelCommand::Ask(_))));
        // The player's own: they are asked.
        let (travel, out) = plan(HandBacks::Ask, true);
        let t = travel.trip.as_ref().expect("waiting on the answer");
        assert!(matches!(t.step, Some(Step::Ask)));
        assert!(out.iter().any(
            |c| matches!(c, TravelCommand::Ask(q) if q.starts_with("Leave the Training Academy"))
        ));
    }

    #[test]
    fn a_plan_that_comes_while_there_is_no_body_is_taken_up_once_there_is() {
        let at = Position::new(
            CellId(0x019E_0114),
            Frame::new(Vec3::new(10.0, -40.0, 0.0), Quat::IDENTITY),
        );
        let mut travel = Travel::new(StoreFiles::default());
        let mut planning = Planning::held();
        planning.give(Answer::Journey {
            id: 5,
            to: Some(at),
            journey: None,
            first: None,
            why: Some(crate::planning::PLANNING_FAILED.to_owned()),
        });
        travel.planning = Some(planning);
        let mut trip = Trip::new(
            1,
            Purpose::Go,
            TravelTarget::Place(at),
            TravelAllow::default(),
            0.0,
            0.0,
        );
        trip.step = Some(Step::Planning {
            job: 5,
            reason: None,
        });
        travel.trip = Some(trip);
        let mut out = Vec::new();
        // A frame with no body (between death and coming back alive, or in portal space).
        travel.answers(None, &mut out);
        assert!(travel.trip.is_some(), "with no body, the plan waits");
        // The next with one: the answer is taken up, and a failed plan ends the journey.
        travel.answers(Some(&at), &mut out);
        assert!(travel.trip.is_none(), "the journey ends");
        assert!(
            travel.events.iter().any(|e| matches!(
                e,
                TravelEvent::Failed { reason, .. } if reason == crate::planning::PLANNING_FAILED
            )),
            "{:?}",
            travel.events
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
    fn a_place_package_taken_as_text_is_hints_in_every_world_store_and_never_saved() {
        let mut t = Travel::new(StoreFiles::default());
        // What the documents say they know does not matter: a package is hints.
        let portals = "dereth-navigation-store\t1\nworld\tDereth\n\
            portal\tpublished\t2068\tDrudge Hideout\t0xA9B30002 5.4 29.5 94.0\t0x019E0114 10.0 -40.0 0.0\t0\t0\t0\t-\t-\t-\n";
        let places = "dereth-places-store\t1\nworld\tDereth\n\
            place\tlifestone\tpublished\t509\tLife Stone\t0xA9B40011 10.000 10.000 0.000\t0\t0\t-\t-\n";
        assert_eq!(t.take_hints(portals), Ok(1));
        assert_eq!(t.take_hints(places), Ok(1));
        t.note_world("A world");
        let store = t.store();
        assert_eq!(store.portals().len(), 1);
        assert_eq!(store.portals()[0].source, EdgeSource::Hint);
        assert!(
            store.without(EdgeSource::Hint).portals().is_empty(),
            "nothing of it is saved"
        );
        let known = t.places().places();
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].source, EdgeSource::Hint);
        assert!(t.take_hints("not a store").is_err());
    }

    #[test]
    fn a_refused_published_portal_stays_out_of_routes_in_the_next_session_kept_in_the_folder() {
        let dir =
            std::env::temp_dir().join(format!("dereth-travel-refusal-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let files = || StoreFiles {
            dir: Some(dir.clone()),
            tiles: None,
            hints: None,
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
            shape: None,
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

    /// A walk at a door: the clock turned by hand, half a second a frame, and the routes answered
    /// by hand, as the planning would with the door planned round or not.
    mod at_a_door {
        use super::*;

        pub(super) const DOOR: ObjectId = ObjectId(0x7A9B_401A);

        pub(super) fn at(x: f32) -> Position {
            Position::new(
                CellId(0x0106_0104),
                Frame::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY),
            )
        }

        fn waypoint(x: f32, door: Option<ObjectId>) -> dereth_navigation::Waypoint {
            dereth_navigation::Waypoint {
                position: at(x),
                action: door.map(WaypointAction::OpenDoor),
            }
        }

        /// A route from the body to the far end of the room, the door within reach of the start.
        pub(super) fn through_the_door() -> NavPath {
            NavPath {
                waypoints: vec![
                    waypoint(0.0, None),
                    waypoint(0.1, Some(DOOR)),
                    waypoint(9.0, None),
                ],
                legs: Vec::new(),
                goal_offset: 0.0,
                cuts: Vec::new(),
            }
        }

        /// A route to the far end of the room with no door on it.
        pub(super) fn round_the_door() -> NavPath {
            NavPath {
                waypoints: vec![waypoint(0.0, None), waypoint(9.0, None)],
                legs: Vec::new(),
                goal_offset: 0.0,
                cuts: Vec::new(),
            }
        }

        pub(super) struct Walk {
            pub travel: Travel,
            pub wall: f64,
            pub uses: u32,
        }

        impl Walk {
            /// A journey on foot, stopped at a closed door that its route goes through.
            pub fn new() -> Self {
                let mut travel = Travel::new(StoreFiles::default());
                travel.planning = Some(Planning::held());
                let mut trip = Trip::new(
                    1,
                    Purpose::Go,
                    TravelTarget::Place(at(9.0)),
                    TravelAllow::default(),
                    0.0,
                    0.0,
                );
                trip.step = Some(Step::Walk {
                    target: at(9.0),
                    last: true,
                });
                travel.trip = Some(trip);
                let mut walk = Self {
                    travel,
                    wall: 0.0,
                    uses: 0,
                };
                walk.door_is(DoorState::Closed);
                walk.travel.follow_route(through_the_door(), true);
                walk
            }

            /// The world's objects now: the door, standing as `state` says.
            pub fn door_is(&mut self, state: DoorState) {
                self.travel.set_obstacles(
                    vec![WorldObstacle {
                        obstacle: Obstacle {
                            id: DOOR,
                            position: at(0.1),
                            setup: Arc::new(SetupGeometry::default()),
                            scale: 1.0,
                            kind: ObstacleKind::Door(state),
                        },
                        name: String::from("Door"),
                    }],
                    Some(&at(0.0)),
                );
            }

            /// Whether the journey now plans round the door.
            pub fn planned_round(&self) -> bool {
                self.travel
                    .trip
                    .as_ref()
                    .is_some_and(|t| t.locked.contains(&DOOR.0))
            }

            /// One frame with the body at the start, `route` answering a route asked for.
            pub fn frame(&mut self, route: impl Fn(&Self) -> Walked) {
                self.wall += 0.5;
                self.travel.wall = self.wall;
                self.travel.now = self.wall;
                let mut out = Vec::new();
                self.travel.trip_frame(&at(0.0), true, &mut out);
                // Each route the frame asked for is answered at once, and an answer may ask again.
                while let Some((_, toward)) =
                    self.travel.trip.as_mut().and_then(|t| t.walking.take())
                {
                    let answer = route(self);
                    self.travel.walked(&at(0.0), toward, answer, 0.0, &mut out);
                }
                self.uses += u32::try_from(
                    out.iter()
                        .filter(|c| matches!(c, TravelCommand::Use(d) if *d == DOOR))
                        .count(),
                )
                .unwrap_or(0);
            }

            /// Frames until the journey ends, or a game hour of them: why it ended.
            pub fn until_it_ends(&mut self, route: impl Fn(&Self) -> Walked) -> Option<String> {
                for _ in 0..7200 {
                    self.frame(&route);
                    if self.travel.trip.is_none() {
                        break;
                    }
                }
                self.travel.take_events().into_iter().find_map(|e| match e {
                    TravelEvent::Failed { reason, .. } => Some(reason),
                    _ => None,
                })
            }
        }

        /// The planner's answer with no way round the door: through it unless planned round.
        pub(super) fn only_through_it(walk: &Walk) -> Walked {
            if walk.planned_round() {
                Err(NoPath::Unreachable)
            } else {
                Ok((through_the_door(), true))
            }
        }
    }

    /// Behaviour: plugins.nav.a-door-that-will-not-open-ends-the-journey-and-says-so
    #[test]
    fn a_door_that_never_opens_and_never_answers_ends_the_journey_after_a_bounded_number_of_uses() {
        let mut walk = at_a_door::Walk::new();
        let why = walk.until_it_ends(at_a_door::only_through_it);
        assert_eq!(
            why,
            Some(format!(
                "door_shut: Door did not open after {DOOR_TRIES} uses"
            ))
        );
        assert_eq!(walk.uses, 2 * DOOR_TRIES, "its uses, then its second go");
        assert!(walk.wall < 60.0, "over in {} s", walk.wall);
    }

    /// Behaviour: plugins.nav.a-door-that-will-not-open-ends-the-journey-and-says-so
    #[test]
    fn a_door_the_server_answers_as_locked_is_used_once_and_ends_a_journey_with_no_way_round() {
        let mut walk = at_a_door::Walk::new();
        walk.frame(at_a_door::only_through_it);
        assert_eq!(walk.uses, 1);
        walk.travel.door_answered_locked(at_a_door::DOOR);
        let why = walk.until_it_ends(at_a_door::only_through_it);
        assert_eq!(
            why.as_deref(),
            Some("door_shut: Door is locked, the server says"),
            "names the door and says why it is shut"
        );
        assert_eq!(walk.uses, 1, "a locked door is not tried again");
    }

    /// Behaviour: plugins.nav.a-door-that-will-not-open-ends-the-journey-and-says-so
    #[test]
    fn a_locked_door_with_a_way_round_is_walked_round_and_the_journey_goes_on() {
        let mut walk = at_a_door::Walk::new();
        walk.frame(at_a_door::only_through_it);
        walk.travel.door_answered_locked(at_a_door::DOOR);
        walk.frame(|_| Ok((at_a_door::round_the_door(), true)));
        assert_eq!(walk.uses, 1);
        let t = walk.travel.trip.as_ref().expect("the journey goes on");
        let route = t.follower.as_ref().expect("it walks on").path();
        assert_eq!(route.waypoints.len(), 2, "the route with no door on it");
    }

    /// Colier Mine's door that cannot be activated from where it stands (at x = 5 on the way)
    /// and the levers that work it, as the server publishes them: one behind the body at x = -4,
    /// and a plate just beyond the door at x = 7, nearer but on its far side.
    mod at_a_lever_door {
        use super::*;

        pub(super) const DOOR: ObjectId = ObjectId(0x701A_E06F);
        pub(super) const LEVER: ObjectId = ObjectId(0x701A_E06A);
        pub(super) const FAR_LEVER: ObjectId = ObjectId(0x701A_E070);

        pub(super) fn at(x: f32) -> Position {
            Position::new(
                CellId(0x01AE_0361),
                Frame::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY),
            )
        }

        /// The places: the door's two levers, with where the door stands.
        const PLACES: &str = concat!(
            "dereth-places-store\t1\nworld\tHere\n",
            "place\tlever\tpublished\t2609\tLever\t0x01AE0361 -4.000 0.000 0.000\t0\t0\t-\t",
            "0x01AE0361 5.000 0.000 0.000\n",
            "place\tlever\tpublished\t298\tPressure Plate\t0x01AE0361 7.000 0.000 0.000\t0\t0\t-\t",
            "0x01AE0361 5.000 0.000 0.000\n",
        );

        /// A straight route from `from` to `to`, opening the door on the way when it crosses it.
        pub(super) fn route(from: f32, to: f32) -> NavPath {
            let mut waypoints = vec![dereth_navigation::Waypoint {
                position: at(from),
                action: None,
            }];
            if from.min(to) < 5.0 && 5.0 < from.max(to) {
                waypoints.push(dereth_navigation::Waypoint {
                    position: at(5.0),
                    action: Some(WaypointAction::OpenDoor(DOOR)),
                });
            }
            waypoints.push(dereth_navigation::Waypoint {
                position: at(to),
                action: None,
            });
            NavPath {
                waypoints,
                legs: Vec::new(),
                goal_offset: 0.0,
                cuts: Vec::new(),
            }
        }

        pub(super) struct Walk {
            pub travel: Travel,
            pub wall: f64,
            pub body: f32,
            pub uses: Vec<ObjectId>,
            pub door: DoorState,
        }

        impl Walk {
            /// A journey on foot to x = 9, stopped at the shut door its route goes through.
            pub fn new() -> Self {
                let mut travel = Travel::new(StoreFiles::default());
                assert_eq!(travel.take_published(PLACES), Ok(2));
                travel.note_world("Here@127.0.0.1:9000");
                travel.planning = Some(Planning::held());
                let mut trip = Trip::new(
                    1,
                    Purpose::Go,
                    TravelTarget::Place(at(9.0)),
                    TravelAllow::default(),
                    0.0,
                    0.0,
                );
                trip.step = Some(Step::Walk {
                    target: at(9.0),
                    last: true,
                });
                travel.trip = Some(trip);
                let mut walk = Self {
                    travel,
                    wall: 0.0,
                    body: 4.9,
                    uses: Vec::new(),
                    door: DoorState::Locked,
                };
                walk.objects();
                walk.travel.follow_route(route(4.9, 9.0), true);
                walk
            }

            /// The world's objects now: the door as `self.door` says (a door no use opens is
            /// `Locked` to the client), and the two levers.
            pub fn objects(&mut self) {
                let object = |id: ObjectId, x: f32, kind: ObstacleKind, name: &str| WorldObstacle {
                    obstacle: Obstacle {
                        id,
                        position: at(x),
                        setup: Arc::new(SetupGeometry::default()),
                        scale: 1.0,
                        kind,
                    },
                    name: String::from(name),
                };
                let list = vec![
                    object(DOOR, 5.0, ObstacleKind::Door(self.door), "Door"),
                    object(LEVER, -4.0, ObstacleKind::Solid, "Lever"),
                    object(FAR_LEVER, 7.0, ObstacleKind::Solid, "Pressure Plate"),
                ];
                self.travel.set_obstacles(list, Some(&at(self.body)));
            }

            /// The door as the travel takes it now.
            pub fn door_seen(&self) -> Option<ObstacleKind> {
                self.travel
                    .obstacles
                    .iter()
                    .find(|o| o.obstacle.id == DOOR)
                    .map(|o| o.obstacle.kind)
            }

            /// One frame with the body where it is, `route` answering each route asked for (to
            /// the x it was asked toward).
            pub fn frame(&mut self, route: impl Fn(&Self, f32) -> Walked) {
                self.wall += 0.5;
                self.travel.wall = self.wall;
                self.travel.now = self.wall;
                let mut out = Vec::new();
                self.travel.trip_frame(&at(self.body), true, &mut out);
                while let Some((_, toward)) =
                    self.travel.trip.as_mut().and_then(|t| t.walking.take())
                {
                    let answer = route(self, toward.frame.origin.x);
                    self.travel
                        .walked(&at(self.body), toward, answer, 0.0, &mut out);
                }
                self.uses.extend(out.iter().filter_map(|c| match c {
                    TravelCommand::Use(o) => Some(*o),
                    _ => None,
                }));
            }

            /// The step the journey is on.
            pub fn step(&self) -> Option<Step> {
                self.travel.trip.as_ref().and_then(|t| t.step.clone())
            }
        }

        /// Every route there is from wherever the body stands, except through the door while
        /// the travel plans it shut.
        pub(super) fn anywhere(walk: &Walk, to: f32) -> Walked {
            let crosses = walk.body.min(to) < 5.0 && 5.0 < walk.body.max(to);
            if crosses && walk.door_seen() == Some(ObstacleKind::Door(DoorState::Locked)) {
                Err(NoPath::Unreachable)
            } else {
                Ok((route(walk.body, to), true))
            }
        }
    }

    /// Behaviour: plugins.nav.a-door-no-use-opens-is-opened-by-its-lever
    /// A door that cannot be used, which the server names a lever for, is planned as passable;
    /// at it the walk goes to the nearest lever on its own side (the way to a lever never goes
    /// through its door), uses it (never the door), and once the door stands open walks back
    /// through it.
    #[test]
    fn a_door_no_use_opens_is_opened_by_walking_to_its_lever_and_using_it() {
        let mut walk = at_a_lever_door::Walk::new();
        assert_eq!(
            walk.door_seen(),
            Some(ObstacleKind::Door(DoorState::Closed)),
            "planned as passable, by its lever"
        );
        walk.frame(at_a_lever_door::anywhere);
        assert!(
            matches!(walk.step(), Some(Step::Lever { which: 1, .. })),
            "the plate beyond the door passed over for the lever behind: {:?}",
            walk.step()
        );
        assert!(walk.uses.is_empty(), "nothing used on the way");
        // At the lever: it is used, once, and given time to work.
        walk.body = -1.5;
        walk.frame(at_a_lever_door::anywhere);
        walk.frame(at_a_lever_door::anywhere);
        assert_eq!(walk.uses, [at_a_lever_door::LEVER]);
        // The door swings open: back through it.
        walk.door = DoorState::Open;
        walk.objects();
        walk.frame(at_a_lever_door::anywhere);
        assert!(
            matches!(walk.step(), Some(Step::Walk { last: true, .. })),
            "on with the walk: {:?}",
            walk.step()
        );
        let end = walk
            .travel
            .trip
            .as_ref()
            .and_then(|t| t.follower.as_ref())
            .expect("a route")
            .path()
            .waypoints
            .last()
            .map(|w| w.position.frame.origin.x);
        assert_eq!(end, Some(9.0), "through the door to where it was going");
        assert!(
            !walk.uses.contains(&at_a_lever_door::DOOR),
            "the door is never used"
        );
    }

    /// Behaviour: plugins.nav.a-door-no-use-opens-is-opened-by-its-lever
    /// With no lever a walk reaches (the plate is beyond the door, and the lever behind is cut
    /// off), the door is planned round, and a journey with no way round ends saying so.
    #[test]
    fn a_lever_no_walk_reaches_is_passed_over_and_with_none_the_journey_says_the_door_is_shut() {
        let mut walk = at_a_lever_door::Walk::new();
        let mut why = None;
        for _ in 0..200 {
            walk.frame(|w, to| {
                let round = w
                    .travel
                    .trip
                    .as_ref()
                    .is_some_and(|t| t.locked.contains(&at_a_lever_door::DOOR.0));
                if to < 0.0 || round {
                    Err(NoPath::Unreachable)
                } else {
                    at_a_lever_door::anywhere(w, to)
                }
            });
            if walk.travel.trip.is_none() {
                why = walk.travel.take_events().into_iter().find_map(|e| match e {
                    TravelEvent::Failed { reason, .. } => Some(reason),
                    _ => None,
                });
                break;
            }
        }
        assert_eq!(
            why.as_deref(),
            Some("door_shut: Door has no lever a walk reaches")
        );
        assert!(walk.uses.is_empty(), "nothing was used");
    }

    /// Behaviour: plugins.nav.a-journey-a-door-stops-says-which-door-and-goes-through-once-it-is-opened
    /// A journey a door stops says which door and why: one the server answered as locked, and one
    /// whose levers no walk reaches.
    #[test]
    fn a_journey_a_door_stops_says_which_door_and_why() {
        let shut_door = |travel: &mut Travel| {
            travel.take_events().into_iter().find_map(|e| match e {
                TravelEvent::Failed { door, .. } => door,
                _ => None,
            })
        };
        let mut walk = at_a_door::Walk::new();
        walk.frame(at_a_door::only_through_it);
        walk.travel.door_answered_locked(at_a_door::DOOR);
        let mut door = None;
        for _ in 0..200 {
            walk.frame(at_a_door::only_through_it);
            if walk.travel.trip.is_none() {
                door = shut_door(&mut walk.travel);
                break;
            }
        }
        assert_eq!(
            door,
            Some(ShutDoor {
                object: at_a_door::DOOR,
                why: DoorWhy::Locked
            })
        );
        let mut walk = at_a_lever_door::Walk::new();
        let mut door = None;
        for _ in 0..200 {
            walk.frame(|w, to| {
                let round = w
                    .travel
                    .trip
                    .as_ref()
                    .is_some_and(|t| t.locked.contains(&at_a_lever_door::DOOR.0));
                if to < 0.0 || round {
                    Err(NoPath::Unreachable)
                } else {
                    at_a_lever_door::anywhere(w, to)
                }
            });
            if walk.travel.trip.is_none() {
                door = shut_door(&mut walk.travel);
                break;
            }
        }
        assert_eq!(
            door,
            Some(ShutDoor {
                object: at_a_lever_door::DOOR,
                why: DoorWhy::NoLever
            })
        );
    }

    /// Colier Mine's hole: a ledge on the entrance's floor, 12 metres over the floor below.
    mod hole {
        use super::*;

        pub(super) fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
            Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
        }

        pub(super) fn drop() -> dereth_navigation::DropLink {
            dereth_navigation::DropLink {
                stand: at(0x01AE_0369, 139.0, -95.0, 0.0),
                land: at(0x01AE_024F, 139.0, -96.0, -12.0),
                height: 12.0,
            }
        }

        /// A journey from the ledge, down the hole and on to the floor below; at its drop.
        pub(super) fn at_the_ledge() -> Travel {
            let mut travel = Travel::new(StoreFiles::default());
            travel.planning = Some(Planning::held());
            let goal = at(0x01AE_01FE, 98.0, -28.3, -12.0);
            let mut trip = Trip::new(
                1,
                Purpose::Go,
                TravelTarget::Place(goal),
                TravelAllow::default(),
                0.0,
                0.0,
            );
            trip.goal = Some(goal);
            trip.journey = Some(Journey {
                legs: vec![
                    Leg::Walk { to: drop().stand },
                    Leg::Drop { drop: drop() },
                    Leg::Walk { to: goal },
                ],
                seconds: 20.0,
            });
            trip.leg = 1;
            travel.trip = Some(trip);
            let mut out = Vec::new();
            travel.start_leg(&drop().stand, &mut out);
            assert_eq!(
                out,
                [
                    TravelCommand::Face(180.0),
                    TravelCommand::Walk(true),
                    TravelCommand::Forward(true)
                ],
                "faces where it lands and walks at the ledge"
            );
            travel
        }

        pub(super) fn frame(travel: &mut Travel, wall: f64, p: &Position) -> Vec<TravelCommand> {
            travel.wall = wall;
            let mut out = Vec::new();
            travel.trip_frame(p, true, &mut out);
            out
        }
    }

    /// Behaviour: none (the travel's own carrying out of a drop)
    /// At a drop: faced where it lands and walking at the ledge, the body hops off it with the
    /// least jump; off the ledge its keys go; come to rest near where the drop lands, the journey
    /// walks on.
    #[test]
    fn a_drop_is_hopped_off_and_the_journey_walks_on_from_where_the_body_comes_down() {
        let mut travel = hole::at_the_ledge();
        let ledge = hole::drop().stand;
        assert_eq!(hole::frame(&mut travel, 0.1, &ledge), []);
        assert_eq!(
            hole::frame(&mut travel, 0.35, &ledge),
            [TravelCommand::Jump]
        );
        let falling = hole::at(0x01AE_024F, 139.0, -97.0, -4.0);
        assert_eq!(
            hole::frame(&mut travel, 0.8, &falling),
            [TravelCommand::Forward(false), TravelCommand::Walk(false)],
            "off the ledge, its keys go"
        );
        let down = hole::at(0x01AE_024F, 139.0, -101.6, -12.0);
        for k in 0..4 {
            let _ = hole::frame(&mut travel, 2.0 + f64::from(k) * 0.1, &down);
        }
        let t = travel.trip.as_ref().expect("the journey goes on");
        assert_eq!(t.leg, 2, "on to the walk below");
        assert!(
            matches!(t.step, Some(Step::Walk { .. })),
            "walking on: {:?}",
            t.step
        );
        // The guard leaves the body the least a jump rises, the client's own feather-touch jump.
        assert_eq!(
            dereth_rules::movement::get_jump_height(0.0, 0, 0.0, 1.0),
            dereth_navigation::LEAST_JUMP_RISE
        );
    }

    /// Behaviour: none (the travel's own carrying out of a drop)
    /// A ledge the body cannot leave, hop as it may, ends the journey saying so; one that sets
    /// it down far from where the drop lands has the journey planned again from there.
    #[test]
    fn a_ledge_that_holds_the_body_ends_the_journey_and_one_far_off_plans_again() {
        let mut travel = hole::at_the_ledge();
        let ledge = hole::drop().stand;
        let mut hops = 0;
        let mut wall = 0.0;
        while travel.trip.is_some() && wall < 10.0 {
            wall += 0.1;
            hops += hole::frame(&mut travel, wall, &ledge)
                .iter()
                .filter(|c| **c == TravelCommand::Jump)
                .count();
        }
        assert_eq!(hops, 3);
        assert!(
            travel.events.iter().any(|e| matches!(
                e,
                TravelEvent::Failed { reason, .. } if reason.starts_with("drop_blocked")
            )),
            "{:?}",
            travel.events
        );

        let mut travel = hole::at_the_ledge();
        let _ = hole::frame(&mut travel, 0.35, &ledge);
        let elsewhere = hole::at(0x01AE_024F, 150.0, -80.0, -12.0);
        for k in 0..5 {
            let _ = hole::frame(&mut travel, 1.0 + f64::from(k) * 0.1, &elsewhere);
        }
        let t = travel.trip.as_ref().expect("the journey goes on");
        assert!(
            matches!(&t.step, Some(Step::Planning { reason: Some(r), .. }) if r == "came down elsewhere"),
            "{:?}",
            t.step
        );
    }

    #[test]
    fn what_a_stuck_body_is_said_to_be_near_leaves_out_what_stands_on_another_floor() {
        let mut travel = Travel::new(StoreFiles::default());
        let place = |x: f32, y: f32, z: f32| {
            Position::new(
                CellId(0xA9B3_0100),
                Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
            )
        };
        let creature = |id: u32, name: &str, at: Position| WorldObstacle {
            obstacle: Obstacle {
                id: ObjectId(id),
                position: at,
                setup: Arc::new(SetupGeometry::default()),
                scale: 1.0,
                kind: ObstacleKind::Creature,
            },
            name: String::from(name),
        };
        // The scriveners' shop: one standing a metre away on the body's floor, one a fifth of a
        // metre across but on the floor above.
        let body = place(182.06, 87.95, 116.005);
        travel.set_obstacles(
            vec![
                creature(1, "Scrivener of War Magic", place(182.336, 87.006, 116.005)),
                creature(
                    2,
                    "Scrivener of Creature Magic",
                    place(182.096, 88.145, 119.005),
                ),
            ],
            Some(&body),
        );
        assert_eq!(
            travel.near(&body, 4.0),
            ["creature Scrivener of War Magic at 1.0 m"]
        );
    }

    #[test]
    fn at_a_low_frame_rate_the_last_of_a_turn_is_faced_at_once_not_turned_with_a_key() {
        let mut travel = Travel::new(StoreFiles::default());
        travel.planning = Some(Planning::held());
        let mut trip = Trip::new(
            1,
            Purpose::Go,
            TravelTarget::Place(at_a_door::at(9.0)),
            TravelAllow::default(),
            0.0,
            0.0,
        );
        trip.step = Some(Step::Walk {
            target: at_a_door::at(9.0),
            last: true,
        });
        travel.trip = Some(trip);
        travel.follow_route(at_a_door::round_the_door(), true);
        // East along the room, facing ten degrees short of it, at five frames a second: a turn
        // key held one frame would turn the body 27 degrees.
        let facing = |degrees: f32| {
            let mut p = at_a_door::at(0.0);
            dereth_primitives::frame::set_heading(&mut p.frame, degrees);
            p
        };
        let mut out = Vec::new();
        travel.follow(&facing(80.0), &mut out);
        out.clear();
        travel.now = 0.2;
        travel.follow(&facing(80.0), &mut out);
        assert!(
            out.iter()
                .any(|c| matches!(c, TravelCommand::Face(h) if (h - 90.0).abs() < 0.5)),
            "{out:?}"
        );
        assert!(
            !out.iter()
                .any(|c| matches!(c, TravelCommand::Turn(Some(_)))),
            "{out:?}"
        );
    }

    /// The Training Academy's Central Courtyard: a walk-in portal in the hall at `0x86020273`
    /// that sets the body down 24 metres off, at `0x8602021E`.
    mod courtyard {
        use super::*;

        pub(super) fn at(cell: u32, x: f32, y: f32) -> Position {
            Position::new(
                CellId(cell),
                Frame::new(Vec3::new(x, y, 0.005), Quat::IDENTITY),
            )
        }

        pub(super) fn portal() -> dereth_navigation::PortalEdge {
            dereth_navigation::PortalEdge {
                source: EdgeSource::Published,
                wcid: Some(31061),
                name: String::from("Central Courtyard"),
                from: at(0x8602_0273, 70.0, -40.0),
                to: Some(at(0x8602_021E, 50.0, -54.0)),
                verified: 0,
                uses: 0,
                failures: 0,
                levels: (None, None),
                retired: None,
                requires: Vec::new(),
                shape: None,
            }
        }

        pub(super) fn sighting() -> PortalSighting {
            let e = portal();
            PortalSighting {
                wcid: e.wcid,
                name: e.name,
                position: e.from,
                shape: None,
            }
        }

        /// A journey for `character` from the hall across the portal to the shop, on its first
        /// leg, the walk to the portal; the store knowing the portal, as the server publishes it.
        pub(super) fn journey(character: &str) -> Travel {
            let mut travel = Travel::new(StoreFiles::default());
            travel.planning = Some(Planning::held());
            travel.character = CharacterState {
                name: character.to_owned(),
                level: Some(40),
                ..CharacterState::default()
            };
            travel.portals.merge([portal()]);
            let shop = at(0x8602_032A, 70.7, -59.1);
            let mut trip = Trip::new(
                1,
                Purpose::Go,
                TravelTarget::Place(shop),
                TravelAllow::default(),
                0.0,
                0.0,
            );
            trip.goal = Some(shop);
            trip.journey = Some(Journey {
                legs: vec![
                    Leg::Walk { to: portal().from },
                    Leg::Portal { edge: portal() },
                    Leg::Walk { to: shop },
                ],
                seconds: 17.0,
            });
            trip.step = Some(Step::Walk {
                target: portal().from,
                last: true,
            });
            trip.last = Some(at(0x8602_0273, 69.3, -39.5));
            travel.trip = Some(trip);
            travel
        }

        pub(super) fn failed(travel: &Travel) -> Option<String> {
            travel.events.iter().find_map(|e| match e {
                TravelEvent::Failed { reason, .. } => Some(reason.clone()),
                _ => None,
            })
        }
    }

    /// Behaviour: none (the travel's own handling of the server's word)
    /// The server refuses the Courtyard, saying a quest is wanted: the journey does without it at
    /// once and says why, and the character's later journeys leave it out, saying why, while
    /// another character's still count on it.
    #[test]
    fn a_portal_the_server_refuses_saying_why_is_done_without_and_kept_from_that_character() {
        let mut travel = courtyard::journey("+Climber");
        let hall = courtyard::at(0x8602_0273, 70.0, -40.0);
        let mut out = Vec::new();
        // In the portal on the walk to it: the server's word arrives.
        assert!(travel.notice(&hall, Some(0x0474), &mut out));
        let why = "You must complete a quest to interact with that portal.";
        assert!(
            travel.events.iter().any(|e| matches!(
                e,
                TravelEvent::Learned { what: Learned::Refusal { name, reason }, .. }
                    if name == "Central Courtyard" && reason == why
            )),
            "{:?}",
            travel.events
        );
        assert_eq!(
            travel
                .portals
                .refused_for("+Climber", &courtyard::portal(), unix_now()),
            Some(why)
        );
        let t = travel
            .trip
            .as_ref()
            .expect("the journey goes on without it");
        assert_eq!(t.stats.refusals, 1);
        assert!(
            matches!(&t.step, Some(Step::Planning { reason: Some(r), .. }) if r.ends_with(why)),
            "planned again, saying why: {:?}",
            t.step
        );
        // Planned again: the portal is no way for this character, and the planning is told why.
        match travel.planning.as_ref().and_then(Planning::last_asked) {
            Some(Job::Journey {
                portals, refused, ..
            }) => {
                assert!(portals.iter().all(|e| e.name != "Central Courtyard"));
                assert_eq!(
                    refused
                        .iter()
                        .map(|(e, r)| (e.name.as_str(), r.to_string()))
                        .collect::<Vec<_>>(),
                    [("Central Courtyard", format!("refused before: {why}"))]
                );
            }
            j => panic!("no journey asked for: {j:?}"),
        }
        // Another character is not refused it.
        let mut other = courtyard::journey("+Other");
        other.portals = travel.portals.clone();
        let mut out = Vec::new();
        other.plan_trip(&hall, &mut out, None);
        match other.planning.as_ref().and_then(Planning::last_asked) {
            Some(Job::Journey {
                portals, refused, ..
            }) => {
                assert!(portals.iter().any(|e| e.name == "Central Courtyard"));
                assert!(refused.is_empty());
            }
            j => panic!("no journey asked for: {j:?}"),
        }
        // Once it has used the portal, it is the character's again.
        travel
            .portals
            .forget_refusal_for("+Climber", &courtyard::sighting());
        assert_eq!(
            travel
                .portals
                .refused_for("+Climber", &courtyard::portal(), unix_now()),
            None
        );
    }

    /// Behaviour: none (the travel's own handling of the server's word)
    /// A refusal that passes (teleported too recently) ends the journey at once, saying so, and
    /// keeps nothing against the portal; a word that is no portal's refusal is let be.
    #[test]
    fn a_passing_refusal_ends_the_journey_saying_so_and_is_not_kept() {
        let mut travel = courtyard::journey("+Climber");
        let hall = courtyard::at(0x8602_0273, 70.0, -40.0);
        let mut out = Vec::new();
        assert!(!travel.notice(&hall, Some(crate::travel::FIZZLED), &mut out));
        assert!(travel.trip.is_some());
        assert!(travel.notice(&hall, Some(0x04A7), &mut out));
        assert_eq!(
            courtyard::failed(&travel).as_deref(),
            Some("You have been teleported too recently!")
        );
        assert!(travel.trip.is_none());
        assert_eq!(
            travel
                .portals
                .refused_for("+Climber", &courtyard::portal(), unix_now()),
            None
        );
    }

    /// Behaviour: none (the travel's own handling of a walk-in portal)
    /// Walking to the Courtyard, the body walks into it and is set down 24 metres off, where it
    /// sends: that is the portal taken, learned, and the journey goes on from there; not a body
    /// stuck on its walk.
    #[test]
    fn set_down_where_a_walk_in_portal_sends_on_the_walk_to_it_is_the_portal_taken() {
        let mut travel = courtyard::journey("+Climber");
        travel.portals.learn_refusal_for(
            "+Climber",
            &courtyard::sighting(),
            "You must complete a quest to interact with that portal.",
            unix_now(),
        );
        let landed = courtyard::at(0x8602_021E, 50.0, -54.0);
        let mut out = Vec::new();
        assert!(!travel.notice(&landed, None, &mut out));
        let t = travel.trip.as_ref().expect("the journey");
        assert!(
            matches!(
                &t.step,
                Some(Step::Wait { what: Awaited::WalkedInto(s), .. }) if s.name == "Central Courtyard"
            ),
            "{:?}",
            t.step
        );
        assert_eq!(t.stats.walked, 0.0, "no walk of 24 metres");
        // Out of portal space: landed, learned, and planned on from there.
        travel.wall = 1.0;
        travel.trip_frame(&landed, true, &mut out);
        let t = travel.trip.as_ref().expect("the journey goes on");
        assert_eq!(t.stats.portal_uses, 1);
        assert_eq!(t.stats.stuck, 0);
        assert!(
            matches!(&t.step, Some(Step::Planning { reason: Some(r), .. }) if r == "walked into portal Central Courtyard"),
            "planned on from where it landed: {:?}",
            t.step
        );
        assert_eq!(
            travel
                .portals
                .refused_for("+Climber", &courtyard::portal(), unix_now()),
            None,
            "a use undoes the refusal"
        );
        assert!(
            travel.events.iter().any(|e| matches!(
                e,
                TravelEvent::Learned { what: Learned::Portal { name, .. }, .. } if name == "Central Courtyard"
            )),
            "{:?}",
            travel.events
        );
    }

    /// Behaviour: plugins.nav.a-door-that-will-not-open-ends-the-journey-and-says-so
    #[test]
    fn a_door_slow_to_open_opens_on_its_second_go_and_the_walk_goes_through_it() {
        let mut walk = at_a_door::Walk::new();
        while walk.uses <= DOOR_TRIES {
            walk.frame(at_a_door::only_through_it);
            assert!(walk.wall < 60.0, "its second go never began");
        }
        assert_eq!(walk.uses, DOOR_TRIES + 1, "the first use of its second go");
        walk.door_is(DoorState::Open);
        walk.frame(at_a_door::only_through_it);
        walk.frame(at_a_door::only_through_it);
        let t = walk.travel.trip.as_ref().expect("the journey goes on");
        assert!(t.door.is_none(), "the walk is through with the door");
        assert_eq!(t.follower.as_ref().map(Follower::next_waypoint), Some(2));
        assert_eq!(walk.uses, DOOR_TRIES + 1, "and uses it no more");
    }

    /// Behaviour: plugins.nav.a-portal-no-walk-reaches-is-planned-round
    /// A walk to a portal that the land does not join to the body plans the journey again without
    /// that portal; with no other way, the journey ends saying which portal no walk reaches.
    #[test]
    fn a_portal_no_walk_reaches_is_planned_round_and_named_when_nothing_else_leads_on() {
        let land = |x: f32, z: f32| {
            Position::new(
                CellId(0x9795_003C),
                Frame::new(Vec3::new(x, 75.0, z), Quat::IDENTITY),
            )
        };
        let (here, stand, dungeon) = (
            land(180.0, 76.3),
            Position::new(
                CellId(0x9395_0023),
                Frame::new(Vec3::new(102.4, 65.5, 224.0), Quat::IDENTITY),
            ),
            Position::new(
                CellId(0x01D4_01B1),
                Frame::new(Vec3::new(61.8, -22.5, 30.0), Quat::IDENTITY),
            ),
        );
        let tower = dereth_navigation::PortalEdge {
            source: EdgeSource::Published,
            wcid: Some(375),
            name: String::from("Bellig Tower"),
            from: stand,
            to: Some(dungeon),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        };
        let mut travel = Travel::new(StoreFiles::default());
        travel.planning = Some(Planning::held());
        travel.portals.merge([tower.clone()]);
        let mut trip = Trip::new(
            1,
            Purpose::Go,
            TravelTarget::Place(dungeon),
            TravelAllow::default(),
            0.0,
            0.0,
        );
        trip.goal = Some(dungeon);
        trip.journey = Some(Journey {
            legs: vec![
                Leg::Walk { to: stand },
                Leg::Portal { edge: tower },
                Leg::Walk { to: dungeon },
            ],
            seconds: 60.0,
        });
        travel.trip = Some(trip);
        let mut out = Vec::new();
        travel.walked(&here, stand, Err(NoPath::Unreachable), 1.0, &mut out);
        let t = travel.trip.as_ref().expect("the journey goes on");
        assert_eq!(t.unwalkable, [stand], "learned");
        match travel.planning.as_ref().and_then(Planning::last_asked) {
            Some(Job::Journey { portals, .. }) => assert!(
                portals.iter().all(|e| e.name != "Bellig Tower"),
                "planned again without it: {portals:?}"
            ),
            other => panic!("planned again: {other:?}"),
        }
        // Nothing else leads there: the journey ends, naming the portal.
        travel.planned(
            &here,
            Some(dungeon),
            None,
            None,
            Some(format!("{UNWALKABLE} Bellig Tower")),
            &mut out,
        );
        assert!(travel.trip.is_none(), "over");
        let why = travel.take_events().into_iter().find_map(|e| match e {
            TravelEvent::Failed { reason, .. } => Some(reason),
            _ => None,
        });
        assert_eq!(why.as_deref(), Some("no walk reaches Bellig Tower"));
    }

    /// Behaviour: plugins.nav.a-portal-no-walk-reaches-is-planned-round
    /// What a trip learned no walk reaches holds for the land the body walked on: set down
    /// somewhere new by a portal, it is forgotten.
    #[test]
    fn a_portal_no_walk_reached_is_tried_again_once_a_portal_sets_the_body_down_elsewhere() {
        let at = |cell: u32, x: f32, z: f32| {
            Position::new(
                CellId(cell),
                Frame::new(Vec3::new(x, 65.0, z), Quat::IDENTITY),
            )
        };
        let (here, plateau, goal) = (
            at(0x9395_001E, 70.0, 114.0),
            at(0x9395_0023, 98.4, 224.0),
            at(0x01D4_021B, 40.0, 114.0),
        );
        let mut travel = Travel::new(StoreFiles::default());
        travel.planning = Some(Planning::held());
        let mut trip = Trip::new(
            1,
            Purpose::Go,
            TravelTarget::Place(goal),
            TravelAllow::default(),
            0.0,
            0.0,
        );
        trip.goal = Some(goal);
        trip.unwalkable.push(at(0x9395_0023, 102.4, 224.0));
        trip.journey = Some(Journey {
            legs: vec![
                Leg::Portal {
                    edge: dereth_navigation::PortalEdge {
                        source: EdgeSource::Published,
                        wcid: Some(28_000),
                        name: String::from("Bellig Top"),
                        from: here,
                        to: Some(plateau),
                        verified: 0,
                        uses: 0,
                        failures: 0,
                        levels: (None, None),
                        retired: None,
                        requires: Vec::new(),
                        shape: None,
                    },
                },
                Leg::Walk { to: goal },
            ],
            seconds: 60.0,
        });
        travel.trip = Some(trip);
        let sighting = PortalSighting {
            wcid: Some(28_000),
            name: String::from("Bellig Top"),
            position: here,
            shape: None,
        };
        let watch = TeleportWatch {
            before: here,
            since: 0.0,
            wait: PORTAL_WAIT,
            to: None,
        };
        travel.wall = 1.0;
        let mut out = Vec::new();
        travel.wait_frame(&plateau, true, &Awaited::Portal(sighting), &watch, &mut out);
        let t = travel.trip.as_ref().expect("the journey goes on");
        assert!(t.unwalkable.is_empty(), "{:?}", t.unwalkable);
    }

    /// Behaviour: plugins.nav.a-door-no-use-opens-is-planned-round-at-once
    #[test]
    fn a_door_that_says_it_cannot_be_used_is_planned_round_at_once() {
        assert_eq!(door_state(true, Some(USEABLE_NO)), DoorState::Open);
        assert_eq!(door_state(false, None), DoorState::Closed);
        assert_eq!(door_state(false, Some(0x0000_0002)), DoorState::Closed);
        assert_eq!(
            door_state(false, Some(USEABLE_NO)),
            DoorState::Locked,
            "a prison door a lever opens"
        );
    }

    /// Behaviour: plugins.nav.a-walk-ends-short-of-a-portal-it-is-not-taking
    /// A route that ends on a portal the journey is not taking ends beyond the portal's own
    /// reach, by a margin; six meters short when what the portal's body is is not known; and not
    /// short at all of a portal that a body is never taken by touching. The walk to a portal the
    /// journey uses goes onto it.
    #[test]
    fn a_walk_ends_short_of_a_portal_it_is_not_taking_by_its_own_reach() {
        let at = |x: f32| {
            Position::new(
                CellId(0x01D4_021B),
                Frame::new(Vec3::new(x, -20.0, 114.0), Quat::IDENTITY),
            )
        };
        let top = dereth_navigation::PortalEdge {
            source: EdgeSource::Published,
            wcid: Some(28_000),
            name: String::from("Bellig Top"),
            from: at(40.0),
            to: Some(Position::new(
                CellId(0x9395_0023),
                Frame::new(Vec3::new(98.4, 67.7, 224.0), Quat::IDENTITY),
            )),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        };
        // A straight route of a place a meter, from 0 to the portal at 40.
        let route = || NavPath {
            waypoints: vec![
                dereth_navigation::Waypoint {
                    position: at(0.0),
                    action: None,
                },
                dereth_navigation::Waypoint {
                    position: at(40.0),
                    action: None,
                },
            ],
            legs: vec![(1..=40).map(|i| at(i as f32)).collect()],
            goal_offset: 0.0,
            cuts: Vec::new(),
        };
        // The common portal's body; its reach for a human, as measured from it.
        let common = PortalShape {
            setup: 0x0200_01B3,
            scale: 1.0,
            touches: true,
        };
        let reach = 1.8355;
        let end_of = |journey: Vec<Leg>, shape: Option<PortalShape>| {
            let mut travel = Travel::new(StoreFiles::default());
            let mut edge = top.clone();
            edge.shape = shape;
            travel.portals.merge([edge]);
            travel
                .reaches
                .insert((common.setup, common.scale.to_bits()), Some(reach));
            let mut trip = Trip::new(
                1,
                Purpose::Go,
                TravelTarget::Place(at(40.0)),
                TravelAllow::default(),
                0.0,
                0.0,
            );
            trip.journey = Some(Journey {
                legs: journey,
                seconds: 30.0,
            });
            travel.trip = Some(trip);
            travel.follow_route(route(), true);
            let t = travel.trip.as_ref().expect("a trip");
            let w = &t.follower.as_ref().expect("following").path().waypoints;
            w.last().expect("an end").position.frame.origin.x
        };
        let walk = || vec![Leg::Walk { to: at(40.0) }];
        let keep = reach + PORTAL_MARGIN;
        let short = 40.0 - end_of(walk(), Some(common));
        assert!(
            (keep..keep + 1.5).contains(&short),
            "ends beyond the portal's reach: {short} m short of it"
        );
        let unknown = 40.0 - end_of(walk(), None);
        assert!(
            (PORTAL_KEEP_OFF..PORTAL_KEEP_OFF + 1.5).contains(&unknown),
            "with its body not known, six meters short: {unknown} m"
        );
        let untouched = PortalShape {
            touches: false,
            ..common
        };
        assert_eq!(
            end_of(walk(), Some(untouched)),
            40.0,
            "a portal no body is taken by touching is walked up to"
        );
        assert_eq!(
            end_of(
                vec![
                    Leg::Walk { to: at(40.0) },
                    Leg::Portal { edge: top.clone() }
                ],
                Some(common)
            ),
            40.0,
            "the walk to a portal the journey takes goes onto it"
        );
    }

    /// Behaviour: plugins.nav.a-journey-reaches-land-walled-off-from-here-through-the-portal-that-leads-in
    /// A journey that is all walking, whose walk finds no way, says no portal known leads there,
    /// and what the client knows of the world's portals: none, or how many from where.
    #[test]
    fn a_journey_on_foot_that_finds_no_walk_says_what_portals_are_known() {
        let ends = |known: Option<dereth_navigation::PortalEdge>| {
            let mut walk = at_a_door::Walk::new();
            if let Some(e) = known {
                walk.travel.portals.merge([e]);
            }
            let t = walk.travel.trip.as_mut().expect("a journey");
            t.journey = Some(Journey {
                legs: vec![Leg::Walk {
                    to: at_a_door::at(9.0),
                }],
                seconds: 5.0,
            });
            t.follower = None;
            t.walking = Some((1, at_a_door::at(9.0)));
            walk.until_it_ends(|_| Err(NoPath::Unreachable))
        };
        assert_eq!(
            ends(None).as_deref(),
            Some(concat!(
                "no_route: no walk was found there, and no portal known leads there ",
                "(no portals known: the server publishes none, and no era package is loaded)"
            ))
        );
        let learned = dereth_navigation::PortalEdge {
            source: EdgeSource::Learned,
            wcid: Some(1),
            name: String::from("Somewhere"),
            from: at_a_door::at(1.0),
            to: None,
            verified: 0,
            uses: 1,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        };
        assert_eq!(
            ends(Some(learned)).as_deref(),
            Some(concat!(
                "no_route: no walk was found there, and no portal known leads there ",
                "(portals known: 1 learned)"
            ))
        );
    }

    /// Behaviour: plugins.nav.a-door-that-will-not-open-ends-the-journey-and-says-so
    #[test]
    fn a_walk_with_no_route_and_no_door_shut_on_it_still_fails_as_no_route() {
        let mut walk = at_a_door::Walk::new();
        let t = walk.travel.trip.as_mut().expect("a journey");
        t.follower = None;
        t.walking = Some((1, at_a_door::at(9.0)));
        let why = walk.until_it_ends(|_| Err(NoPath::Unreachable));
        assert_eq!(why.as_deref(), Some("no_route"));
        assert_eq!(walk.uses, 0);
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
            refused: String::from("needs the player to agree to leave the Training Academy"),
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
                    portal_refused: None,
                    object: &object,
                    hand_backs: Vec::new(),
                    holds: &holds,
                    answer: None,
                    body: Vec::new(),
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

    /// Behaviour: plugins.nav.landblock-tiles-kept-between-sessions-are-read-back-not-built-again
    /// The planner's kept tiles are kept apart for each set of the data it reads: a world whose
    /// data patch lays its own records over the cell file keeps its own, and so does the same
    /// world once its patch has changed.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn kept_tiles_are_kept_apart_for_a_worlds_data_patch_and_for_each_change_to_it() {
        use dereth_dat::overlay::{OverlayDir, OverlayWriter};
        let scratch = dereth_dat::testing::ScratchDir::new("nav-cache-overlay").expect("scratch");
        let root = scratch.path().join("navigation");
        let store = dereth_dat::testing::open_store_or_fail();
        let plain = CacheTiles::for_store(&root, &store).dir;
        let overlay = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
        let base = store.cell().base();
        let patch = |version: u16| {
            let mut w = OverlayWriter::open_or_create(
                &overlay.container(dereth_dat::ModernDat::Cell),
                &base,
                "cell.dat",
                "a world",
                1,
            )
            .expect("the overlay");
            let outcome = w
                .save(
                    &base,
                    DataId(0xA9B4_FFFE),
                    &[1, 2, 3, u8::try_from(version).unwrap_or(0)],
                    version,
                    1_000_000 + u32::from(version),
                    u32::from(version),
                )
                .expect("the world's record");
            assert!(
                !matches!(outcome, dereth_dat::SaveOutcome::RefusedOlderIteration),
                "{outcome:?}"
            );
            w.flush(u32::from(version)).expect("flushed");
        };
        patch(1);
        let patched = store
            .clone()
            .with_overlay(&overlay, Some("a world"))
            .expect("the overlay opens over the cell file");
        let first = CacheTiles::for_store(&root, &patched).dir;
        assert_ne!(first, plain, "a world's patch keeps its own tiles");
        patch(2);
        let changed = store
            .clone()
            .with_overlay(&overlay, Some("a world"))
            .expect("the overlay opens over the cell file");
        let second = CacheTiles::for_store(&root, &changed).dir;
        assert_ne!(second, first, "a changed patch keeps its own again");
        assert_eq!(
            CacheTiles::for_store(&root, &store).dir,
            plain,
            "the plain files keep theirs"
        );
    }

    /// Behaviour: plugins.nav.a-route-to-an-upstairs-room-ends-on-its-floor
    /// A walk asked to go to a room upstairs (the bowyer's in Holtburg, its floor 3 m above the
    /// shop's) goes up to that room's floor, not to the ground floor under it, where the
    /// building's frame stands.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_route_to_an_upstairs_room_goes_up_to_its_floor() {
        use crate::planning::{Answer, Goal, Job, Planning};
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land"));
        let upstairs = CellId(0xA9B4_0123);
        land.load_block_cells(block_of(upstairs));
        let middle = middle(&land, upstairs).expect("the room's middle");
        assert!(
            (middle.frame.origin.z - 69.0).abs() < 0.1,
            "on the room's floor: {:?}",
            middle.frame.origin
        );
        let shop = Position::new(
            CellId(0xA9B4_0121),
            Frame::new(Vec3::new(101.5, 135.6, 66.005), Quat::IDENTITY),
        );
        let mut planning = Planning::in_frame(&land, &store, None).expect("a planner");
        planning.ask(Job::Route {
            id: 1,
            from: shop,
            to: Goal::Cell(upstairs),
            reach: None,
            snap: None,
        });
        match planning.answers_waited().as_slice() {
            [Answer::Route {
                route: Ok((path, _)),
                ..
            }] => {
                let end = path.waypoints.last().expect("an end").position;
                assert_eq!(end.cell, upstairs, "{path:?}");
                assert!(
                    end.frame.origin.z > 68.5,
                    "upstairs: {:?}",
                    end.frame.origin
                );
            }
            other => panic!("{other:?}"),
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
            Planning::in_frame(&land, &store, None)
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
                travel.planning = Some(Planning::in_frame(&land, &store, None).expect("a planner"));
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
