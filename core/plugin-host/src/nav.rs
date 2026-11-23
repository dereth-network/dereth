//! Navigation: a plugin's movement of the player's character.
//!
//! A plugin never holds a key. It asks for a movement and the host steers the character the way a
//! player at the keyboard would: it turns the character in place toward the target with the
//! client's turn-to-heading movement, then sets the run lock (autorun) and lets it go on arrival,
//! stopping to turn again when the target drifts off to the side, unless it will pass within range
//! of it as it runs. The server sees ordinary
//! movement. A path of the plugin's own points (`nav.move_along`) is run with the forward key
//! instead, steered as a journey's walk is (the same follower), and waypoint routes
//! (`dereth.nav`) are walked so; the way round what stands in between is the client's own
//! planner's, asked for as a journey or a path (below).
//!
//! One movement runs at a time across all plugins. Each is a promise:
//!
//! | action | completes | fails |
//! |---|---|---|
//! | `nav.move_to(target, range)` | within `range` metres of the target (on the ground) | `stuck` (no progress for 6 s), `timeout`, `target_gone`, `no_position`, `interrupted`, `stopped`, `replaced` |
//! | `nav.move_along(points, range)` | having come within `range` of each point in turn, opening a shut door on the way | `stuck` (no nearer for 6 s), `door_shut: …`, `no_position`, `interrupted`, `stopped`, `replaced` |
//! | `nav.follow(object, range)` | never; it keeps within `range` until stopped | `lost` (the object gone for 3 s), `interrupted`, `stopped`, `replaced` |
//! | `nav.face(heading)` | facing within 5° of the heading | `timeout` (3 s), `interrupted`, `stopped`, `replaced` |
//! | `nav.jump{ ms, heading, walk, direction }` | a frame after the leap | `timeout` (3 s to face), `interrupted`, `stopped`, `replaced` |
//! | `nav.motion(kind, ms, { walk })` | once the control has been held `ms` | `interrupted`, `stopped`, `replaced` |
//!
//! A jump and a held motion press the game's own movement controls, as the client's own steering:
//! a jump faces its heading first, then presses the jump, and while it charges holds the walk key
//! and the direction it leaps in (a jump charged standing still goes the way the controls held
//! during its charge say), and lets go of the jump after `ms` (the power bar is full at a second).
//! Whatever the host holds is let go when the movement ends, however it ends; a jump still
//! charging then is put down without a leap.
//!
//! **The player and the server come first.** A movement fails `interrupted` the frame the player
//! moves the character themselves (a movement key, the run lock, a jump), and the frame the server
//! starts moving it (the approach to something used or attacked); after the player's own
//! movement, a movement no gesture asked for is refused for [`PLAYER_YIELD_SECS`]. No turn is
//! asked for while the server moves the character.
//!
//! **The run lock is let go only by whoever set it.** The host remembers whether it set the run
//! lock, and for which plugin: a movement's end, `nav.stop()`, and the plugin's unloading or
//! suspension let go of the run lock that plugin set, and nothing else. When the player works the
//! movement keys the run lock is theirs again, and the host never touches it until a plugin sets it
//! anew.
//!
//! **Journeys** (`nav.path_to`) are the client's own: the host asks the client's travel for one,
//! and the travel plans it over walking, the portals this world's store knows and the
//! character's recalls, and carries it out. A journey is a movement like the others: one runs at a
//! time across all plugins, starting one replaces any movement, starting any movement replaces it,
//! and the player's own movement ends it (`interrupted`). Its progress reaches the plugin that
//! asked as `nav.journey_planned`, `nav.journey_leg` and `nav.journey_learned` events, and the
//! host draws it (an arrow to the next leg's end, the way on the map and radar) unless the plugin
//! hides it. `nav.plan` answers the journey without moving.
//!
//! **Paths** (`nav.path`) are the travel's too: the walking route to a target, its corners and the
//! doors on it, answered without moving, as `nav.plan` is. Whether a straight line between two
//! places is clear (`nav.sight`) is the frame's own answer, swept through the body's world.

use dereth_client_runtime::frame_hook::{HookFrame, NavKey, NavRequest};
use dereth_client_runtime::travel::{
    Learned, TravelAllow, TravelEvent, TravelLeg, TravelPlan, TravelRequest, TravelRoute,
    TravelTarget,
};
use dereth_navigation::{FollowConfig, Follower, NavPath, Steer, Waypoint};
use dereth_plugin_api::Value;
use dereth_primitives::{ObjectId, Position};

use crate::actions::Settled;
use crate::overlay::Anchor;
use crate::places;

/// How far off the target's bearing, in degrees, the character may be and still run; beyond it,
/// it stops and turns in place.
pub const TURN_TOLERANCE: f64 = 15.0;
/// Seconds without coming half a metre closer before a move fails as `stuck`.
pub const STUCK_SECS: f64 = 6.0;
/// Seconds a followed object may be out of sight before the follow fails as `lost`.
pub const LOST_SECS: f64 = 3.0;
/// The slowest pace a move is allowed, in metres a second, before it fails as `timeout`.
pub const SLOWEST_PACE: f64 = 2.0;
/// How long a face may take.
pub const FACE_SECS: f64 = 3.0;
/// A move running at its target goes on running without turning when it passes the target by
/// no more than this share of its range: it arrives without the stop.
pub const PASS_WITHIN: f64 = 0.95;
/// How long a turn is given to finish before it is asked for again.
pub const TURN_RETRY_SECS: f64 = 2.0;
/// Seconds after the player moves the character themselves during which a movement no gesture
/// asked for is refused.
pub const PLAYER_YIELD_SECS: f64 = 5.0;
/// The longest a jump is charged: the power bar is full at a second.
pub const JUMP_MAX_SECS: f64 = 1.0;
/// The longest a motion is held in one movement.
pub const MOTION_MAX_SECS: f64 = 10.0;
/// How near a shut door across its way a move along comes before it uses the door, metres, and
/// how far to either side of the way the door may stand.
pub const DOOR_NEAR: f64 = 1.5;
pub const DOOR_ACROSS: f64 = 1.0;
/// Seconds a used door is waited for before it is used again, and how many uses it is given, as
/// a journey's walk gives a door.
pub const DOOR_WAIT: f64 = 4.0;
pub const DOOR_TRIES: u32 = 4;
/// The physics state of an object that bodies pass through: a door standing open.
const ETHEREAL: u32 = 0x0000_0004;
/// The most points one move along takes.
pub const PATH_POINTS: usize = 4096;
/// How many frames what a movement ran with stays held once it has arrived, for the plugin's
/// next movement to take up: one to hear of the arrival, and one more to answer it.
pub const CHAIN_FRAMES: u32 = 2;
/// And at most this long, in seconds, however few frames that is: at a low frame rate the
/// plugin has the one frame it hears of the arrival in.
pub const CHAIN_SECS: f64 = 0.1;

/// A movement.
#[derive(Debug, Clone, PartialEq)]
pub enum Goal {
    MoveTo {
        target: Anchor,
        range: f64,
    },
    /// Run through the plugin's own points in turn, coming within `range` of each.
    MoveAlong {
        points: Vec<Position>,
        range: f64,
    },
    Follow {
        object: ObjectId,
        range: f64,
    },
    Face {
        heading: f64,
    },
    /// Face `heading` (when given), charge the jump for `hold` seconds holding the walk key and
    /// `direction` (when given), then leap.
    Jump {
        heading: Option<f64>,
        hold: f64,
        walk: bool,
        direction: Option<NavKey>,
    },
    /// Hold one movement control for `secs` seconds, with the walk key when `walk`.
    Motion {
        key: NavKey,
        secs: f64,
        walk: bool,
    },
}

impl Goal {
    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self {
            Self::MoveTo { .. } => "nav.move_to",
            Self::MoveAlong { .. } => "nav.move_along",
            Self::Follow { .. } => "nav.follow",
            Self::Face { .. } => "nav.face",
            Self::Jump { .. } => "nav.jump",
            Self::Motion { .. } => "nav.motion",
        }
    }

    /// Whether it runs the character by the run lock.
    const fn uses_run_lock(&self) -> bool {
        matches!(self, Self::MoveTo { .. } | Self::Follow { .. })
    }
}

/// What a movement that arrived ran with, held a moment for the plugin's next.
#[derive(Debug, Clone)]
struct Chain {
    plugin: String,
    frames: u32,
    until: f64,
    /// The steering of a move along that arrived: the next goes on from it.
    follower: Option<Follower>,
}

#[derive(Debug, Clone)]
struct Running {
    plugin: String,
    id: i64,
    goal: Goal,
    started: f64,
    /// The closest the character has come, and when.
    best: Option<(f64, f64)>,
    /// How far the target was when the move began.
    from: Option<f64>,
    /// When the followed object was last seen.
    seen: f64,
    /// The last heading turned to, and when.
    turned: Option<(f64, f64)>,
    /// When the host pressed the movement's controls (a jump's charge, a held motion).
    pressed: Option<f64>,
    /// When a jump leapt.
    leapt: Option<f64>,
    /// A move along's steering, once it has set out.
    follower: Option<Follower>,
    /// The steering of the plugin's move along before this one, which it goes on from.
    before: Option<Follower>,
    /// The shut door a move along is opening: the door, when it was last used, and how many
    /// times.
    door: Option<(ObjectId, f64, u32)>,
}

/// A journey asked of the client's travel.
#[derive(Debug, Clone)]
struct Journey {
    plugin: String,
    /// The plugin's promise.
    id: i64,
    /// The travel's own id for it.
    travel: u64,
    /// Whether the host draws it.
    show: bool,
}

/// The movement in progress, if any.
#[derive(Debug, Default)]
pub struct Nav {
    running: Option<Running>,
    /// The journey in progress, if any.
    journey: Option<Journey>,
    /// Plans asked for and not yet answered: travel id to the plugin and its promise.
    plans: std::collections::BTreeMap<u64, (String, i64)>,
    /// Walking routes asked for (`nav.path`) and not yet answered, the same way.
    routes: std::collections::BTreeMap<u64, (String, i64)>,
    next_travel: u64,
    /// Requests for the travel, sent at the next step.
    travel_out: Vec<TravelRequest>,
    /// The plugin the run lock is on for, when the host set it: only this is let go.
    run_owner: Option<String>,
    /// The run lock a plugin asked for directly (`nav.autorun`), applied at the next step.
    asked_run: Option<(String, bool)>,
    /// A let-go of the run lock the host set, due at the next step.
    release: bool,
    /// When the player last moved the character themselves.
    player_moved: Option<f64>,
    /// The movement controls the host holds for the movement running.
    held: Vec<NavKey>,
    /// The doors the server has answered a use of as locked, until one is seen open: a move along
    /// meeting one does not use it again.
    locked_doors: std::collections::BTreeSet<ObjectId>,
    /// A movement just arrived, what it ran with still held.
    chain: Option<Chain>,
    /// What an arrival held was let go of for another action of the plugin's, which waits a
    /// frame for the character to stand.
    standing_first: bool,
    /// A journey was ended for the movement starting: the travel lets go of the keys later in
    /// this frame, so the movement presses none, and asks no turn, before the next.
    travel_letting_go: bool,
    /// Progress to tell the plugins at the next step: plugin, event, data.
    progress: Vec<(String, &'static str, Value)>,
    /// Controls to let go of at the next step: the movement holding them ended between frames.
    unheld: Vec<NavKey>,
    /// The door each failed journey could not get through, by plugin and promise: what its
    /// `action.failed` carries.
    shut_doors: std::collections::BTreeMap<(String, i64), Value>,
}

/// The appraised property a door's lock is picked against (its resistance, as an appraisal
/// tells it).
const RESIST_LOCKPICK: u32 = 173;

/// What the client knows of `door`: its id, its name, and its lock's resistance to picking once
/// it has been appraised.
fn door_value(world: &dereth_client_model::World, door: ObjectId) -> Value {
    let name = world.weenie(door).map(|w| w.pwd.name.clone());
    let resist = world.appraisal.get(door).and_then(|p| {
        p.tables
            .ints
            .as_ref()?
            .entries
            .iter()
            .find(|(k, _)| *k == RESIST_LOCKPICK)
            .map(|(_, v)| i64::from(*v))
    });
    Value::map()
        .with("object", door.0)
        .with("name", name)
        .with("resist", resist)
}

fn settled(r: &Running, result: Result<Value, String>) -> Settled {
    Settled {
        plugin: r.plugin.clone(),
        id: r.id,
        action: r.goal.action(),
        result,
    }
}

impl Nav {
    /// Start `goal` for `plugin` as action `id`: a movement already running fails `replaced`.
    pub fn start(&mut self, plugin: &str, id: i64, goal: Goal, now: f64) -> Option<Settled> {
        if let Some(s) = self.end_journey("replaced") {
            self.travel_letting_go = true;
            return self.start(plugin, id, goal, now).or(Some(s));
        }
        let old = self.running.take();
        let chain = self.chain.take();
        let along = matches!(goal, Goal::MoveAlong { .. });
        // The plugin's move along before this one, under way or just arrived.
        let before = along
            .then(|| match (&old, &chain) {
                (Some(r), _) if r.plugin == plugin => {
                    r.follower.clone().or_else(|| r.before.clone())
                }
                (None, Some(c)) if c.plugin == plugin => c.follower.clone(),
                _ => None,
            })
            .flatten();
        // Whom the run lock is held for by a movement: only the same plugin's next that runs by
        // it takes it over, and any other has it let go.
        let locked = match (&old, &chain) {
            (Some(r), _) if r.goal.uses_run_lock() => Some(&r.plugin),
            (None, Some(c)) => Some(&c.plugin),
            _ => None,
        };
        if let Some(p) = locked {
            if p != plugin || !goal.uses_run_lock() {
                self.let_go(p);
            }
        }
        if along {
            // It runs with the forward key: a run lock the host set is let go, as for a journey.
            if let Some(owner) = self.run_owner.clone() {
                self.let_go(&owner);
            }
        }
        if before.is_none() {
            self.drop_held();
        }
        self.running = Some(Running {
            plugin: plugin.to_owned(),
            id,
            goal,
            started: now,
            best: None,
            from: None,
            seen: now,
            turned: None,
            pressed: None,
            leapt: None,
            follower: None,
            before,
            door: None,
        });
        old.map(|r| settled(&r, Err("replaced".into())))
    }

    /// The controls held for the movement that just ended between frames are let go at the next
    /// step.
    fn drop_held(&mut self) {
        self.unheld.append(&mut self.held);
    }

    /// Press `key` for the movement running.
    fn press(&mut self, frame: &mut HookFrame<'_>, key: NavKey) {
        frame.nav(NavRequest::Hold(key, true));
        self.held.push(key);
    }

    /// Let go of every control in `keys`: a jump still charging is put down without a leap.
    fn let_go_of(frame: &mut HookFrame<'_>, keys: &mut Vec<NavKey>) {
        for key in keys.drain(..) {
            if key == NavKey::Jump {
                frame.nav(NavRequest::CancelJump);
            } else {
                frame.nav(NavRequest::Hold(key, false));
            }
        }
    }

    /// Let go of what a movement that just arrived ran with.
    fn drop_chain(&mut self) {
        if let Some(c) = self.chain.take() {
            self.let_go(&c.plugin);
            self.drop_held();
        }
    }

    /// `plugin` asks for something other than a movement: what its movement that just arrived
    /// ran with is let go of, and the action waits for that ([`Self::take_standing_first`]).
    pub fn act_instead(&mut self, plugin: &str) {
        if self.chain.as_ref().is_some_and(|c| c.plugin == plugin) {
            self.drop_chain();
            self.standing_first = true;
        }
    }

    /// Whether this frame's action waits for the character to stand, having just been let go of.
    pub fn take_standing_first(&mut self) -> bool {
        std::mem::take(&mut self.standing_first)
    }

    /// Press `key` or let go of it, as `down` says, where it is not so already.
    fn set_held(&mut self, frame: &mut HookFrame<'_>, key: NavKey, down: bool) {
        if self.held.contains(&key) == down {
            return;
        }
        if down {
            self.press(frame, key);
        } else {
            frame.nav(NavRequest::Hold(key, false));
            self.held.retain(|k| *k != key);
        }
    }

    /// Hold the turn key of `turn` (right when true), or neither.
    fn set_turn(&mut self, frame: &mut HookFrame<'_>, turn: Option<bool>) {
        // The one let go first: the game takes the key pressed last.
        if turn != Some(true) {
            self.set_held(frame, NavKey::TurnRight, false);
        }
        if turn != Some(false) {
            self.set_held(frame, NavKey::TurnLeft, false);
        }
        if let Some(right) = turn {
            let key = if right {
                NavKey::TurnRight
            } else {
                NavKey::TurnLeft
            };
            self.set_held(frame, key, true);
        }
    }

    /// The travel's next id.
    fn travel_id(&mut self) -> u64 {
        self.next_travel += 1;
        self.next_travel
    }

    /// End the journey in progress with `reason`, asking the travel to stop it: its promise's
    /// settlement.
    fn end_journey(&mut self, reason: &str) -> Option<Settled> {
        let j = self.journey.take()?;
        self.travel_out.push(TravelRequest::Cancel {
            id: j.travel,
            reason: reason.to_owned(),
        });
        Some(Settled {
            plugin: j.plugin,
            id: j.id,
            action: "nav.path_to",
            result: Err(reason.to_owned()),
        })
    }

    /// Start a journey for `plugin` as action `id`: any movement or journey running fails
    /// `replaced`.
    pub fn start_journey(
        &mut self,
        plugin: &str,
        id: i64,
        target: TravelTarget,
        allow: TravelAllow,
        show: bool,
    ) -> Vec<Settled> {
        let mut out: Vec<Settled> = self.end_journey("replaced").into_iter().collect();
        self.drop_chain();
        if let Some(r) = self.running.take() {
            if let Some(owner) = self.run_owner.clone() {
                self.let_go(&owner);
            }
            self.drop_held();
            out.push(settled(&r, Err("replaced".into())));
        }
        let travel = self.travel_id();
        self.travel_out.push(TravelRequest::Go {
            id: travel,
            target,
            allow,
        });
        self.journey = Some(Journey {
            plugin: plugin.to_owned(),
            id,
            travel,
            show,
        });
        out
    }

    /// Ask the travel for the journey to `target` without moving, for `plugin`'s action `id`.
    pub fn plan(&mut self, plugin: &str, id: i64, target: TravelTarget, allow: TravelAllow) {
        let travel = self.travel_id();
        self.travel_out.push(TravelRequest::Plan {
            id: travel,
            target,
            allow,
        });
        self.plans.insert(travel, (plugin.to_owned(), id));
    }

    /// Ask the travel for the walking route to `target`, for `plugin`'s action `id`.
    pub fn route(&mut self, plugin: &str, id: i64, target: TravelTarget) {
        let travel = self.travel_id();
        self.travel_out
            .push(TravelRequest::Route { id: travel, target });
        self.routes.insert(travel, (plugin.to_owned(), id));
    }

    /// Whether `plugin`'s journey is drawn, and its travel id.
    #[must_use]
    pub fn shown_journey(&self) -> Option<(&str, u64)> {
        self.journey
            .as_ref()
            .filter(|j| j.show)
            .map(|j| (j.plugin.as_str(), j.travel))
    }

    /// Show or hide `plugin`'s journey; false when it has none.
    pub fn show_journey(&mut self, plugin: &str, show: bool) -> bool {
        match self.journey.as_mut().filter(|j| j.plugin == plugin) {
            Some(j) => {
                j.show = show;
                true
            }
            None => false,
        }
    }

    /// Hand the travel what was asked of it this frame.
    pub fn flush_travel(&mut self, frame: &mut HookFrame<'_>) {
        for r in std::mem::take(&mut self.travel_out) {
            frame.travel(r);
        }
    }

    /// Turn what the travel told since the last frame into settlements, and events for the
    /// plugins, with the moves along's progress: `(plugin, event name, data)`.
    pub fn travel_news(
        &mut self,
        frame: &HookFrame<'_>,
    ) -> (Vec<Settled>, Vec<(String, &'static str, Value)>) {
        let mut settled_out = Vec::new();
        let mut events = std::mem::take(&mut self.progress);
        for e in &frame.travel.events {
            match e {
                TravelEvent::Answer { id, plan } => {
                    if let Some((plugin, action)) = self.plans.remove(id) {
                        settled_out.push(Settled {
                            plugin,
                            id: action,
                            action: "nav.plan",
                            result: plan.as_ref().map(plan_value).map_err(Clone::clone),
                        });
                    }
                }
                TravelEvent::Route { id, route } => {
                    if let Some((plugin, action)) = self.routes.remove(id) {
                        settled_out.push(Settled {
                            plugin,
                            id: action,
                            action: "nav.path",
                            result: route.as_ref().map(route_value).map_err(Clone::clone),
                        });
                    }
                }
                TravelEvent::Planned { id, plan, reason } => {
                    if let Some(j) = self.journey.as_ref().filter(|j| j.travel == *id) {
                        let mut v = plan_value(plan).with("id", j.id);
                        if let Some(r) = reason {
                            v = v.with("reason", r.as_str());
                        }
                        events.push((j.plugin.clone(), "nav.journey_planned", v));
                    }
                }
                TravelEvent::Leg { id, index, leg } => {
                    if let Some(j) = self.journey.as_ref().filter(|j| j.travel == *id) {
                        let mut v = leg_value(leg)
                            .with("id", j.id)
                            .with("index", i64::try_from(*index).unwrap_or(0) + 1);
                        // A door: what the client knows of it, for a plugin that opens it itself.
                        if let TravelLeg::Door { object, .. } = leg {
                            let d = door_value(frame.world, *object);
                            v = v
                                .with("name", d.get("name").clone())
                                .with("resist", d.get("resist").clone());
                        }
                        events.push((j.plugin.clone(), "nav.journey_leg", v));
                    }
                }
                TravelEvent::Learned { id, what } => {
                    if let Some(j) = self.journey.as_ref().filter(|j| j.travel == *id) {
                        let v = learned_value(what).with("id", j.id);
                        events.push((j.plugin.clone(), "nav.journey_learned", v));
                    }
                }
                TravelEvent::Arrived {
                    id,
                    distance,
                    stats,
                } => {
                    if let Some(j) = self.journey.take_if(|j| j.travel == *id) {
                        settled_out.push(Settled {
                            plugin: j.plugin,
                            id: j.id,
                            action: "nav.path_to",
                            result: Ok(Value::map()
                                .with("distance", (f64::from(*distance) * 10.0).round() / 10.0)
                                .with("portals", stats.portal_uses)
                                .with("recalls", stats.recalls)
                                .with("replans", stats.replans)
                                .with("walked", (f64::from(stats.walked) * 10.0).round() / 10.0)
                                .with("seconds", (stats.wall_seconds * 10.0).round() / 10.0)),
                        });
                    }
                }
                TravelEvent::Failed {
                    id, reason, door, ..
                } => {
                    if let Some(j) = self.journey.take_if(|j| j.travel == *id) {
                        if let Some(d) = door {
                            self.shut_doors.insert(
                                (j.plugin.clone(), j.id),
                                door_value(frame.world, d.object).with("why", d.why.word()),
                            );
                        }
                        settled_out.push(Settled {
                            plugin: j.plugin,
                            id: j.id,
                            action: "nav.path_to",
                            result: Err(reason.clone()),
                        });
                    }
                }
            }
        }
        (settled_out, events)
    }

    /// The door `plugin`'s failed journey `id` could not get through, once: what its
    /// `action.failed` says of it.
    pub fn take_shut_door(&mut self, plugin: &str, id: i64) -> Option<Value> {
        self.shut_doors.remove(&(plugin.to_owned(), id))
    }

    /// `plugin` stops movement (`nav.stop()`): its running movement fails `stopped`, and the run
    /// lock it set is let go. Another plugin's movement, and a run lock the player set, stay.
    pub fn stop(&mut self, plugin: &str) -> Option<Settled> {
        if self.journey.as_ref().is_some_and(|j| j.plugin == plugin) {
            return self.end_journey("stopped");
        }
        if self.chain.as_ref().is_some_and(|c| c.plugin == plugin) {
            self.drop_chain();
        }
        self.let_go(plugin);
        let r = self.running.take_if(|r| r.plugin == plugin)?;
        self.drop_held();
        Some(settled(&r, Err("stopped".into())))
    }

    /// Forget `plugin`'s movement (it is being unloaded or was suspended): it stops without a
    /// settlement, and the run lock it set is let go at the next step.
    pub fn forget(&mut self, plugin: &str) {
        if self.running.as_ref().is_some_and(|r| r.plugin == plugin) {
            self.running = None;
            self.drop_held();
        }
        if self.journey.as_ref().is_some_and(|j| j.plugin == plugin) {
            let _ = self.end_journey("stopped");
        }
        self.plans.retain(|_, (p, _)| p != plugin);
        self.routes.retain(|_, (p, _)| p != plugin);
        if self.asked_run.as_ref().is_some_and(|(p, _)| p == plugin) {
            self.asked_run = None;
        }
        if self.chain.as_ref().is_some_and(|c| c.plugin == plugin) {
            self.drop_chain();
        }
        self.let_go(plugin);
    }

    fn let_go(&mut self, plugin: &str) {
        if self.run_owner.as_deref() == Some(plugin) {
            self.run_owner = None;
            self.release = true;
        }
    }

    /// `plugin` sets or lets go of the run lock itself (`nav.autorun`), at the next step. Letting
    /// go touches only the run lock `plugin` set, and never another plugin's request.
    pub fn ask_run(&mut self, plugin: &str, on: bool) {
        if on {
            // The run lock is the plugin's own from here, and no arrival's to let go of.
            if self.chain.take().is_some() {
                self.drop_held();
            }
            self.asked_run = Some((plugin.to_owned(), true));
        } else {
            if self.asked_run.as_ref().is_some_and(|(p, _)| p == plugin) {
                self.asked_run = None;
            }
            self.let_go(plugin);
        }
    }

    /// Which plugin is moving the character.
    #[must_use]
    pub fn owner(&self) -> Option<&str> {
        self.running
            .as_ref()
            .map(|r| r.plugin.as_str())
            .or_else(|| self.journey.as_ref().map(|j| j.plugin.as_str()))
    }

    /// Whether `plugin`'s movement `id` is the one running.
    #[must_use]
    pub fn is_running(&self, plugin: &str, id: i64) -> bool {
        self.running
            .as_ref()
            .is_some_and(|r| r.plugin == plugin && r.id == id)
    }

    /// The plugin the run lock is on for, when the host set it.
    #[must_use]
    pub fn run_owner(&self) -> Option<&str> {
        self.run_owner.as_deref()
    }

    /// Whether the player moved the character themselves within [`PLAYER_YIELD_SECS`] of `now`.
    #[must_use]
    pub fn player_recently_moved(&self, now: f64) -> bool {
        self.player_moved
            .is_some_and(|t| now - t < PLAYER_YIELD_SECS)
    }

    /// Set the run lock on for `plugin`, or let go of it when the host set it.
    fn run(&mut self, frame: &mut HookFrame<'_>, plugin: &str, on: bool) {
        if on {
            if !frame.places.autorun() {
                frame.nav(NavRequest::Autorun(true));
            }
            self.run_owner = Some(plugin.to_owned());
        } else if self.run_owner.as_deref() == Some(plugin) {
            if frame.places.autorun() {
                frame.nav(NavRequest::Autorun(false));
            }
            self.run_owner = None;
        }
    }

    fn face(r: &mut Running, frame: &mut HookFrame<'_>, heading: f64, now: f64) {
        let due = r
            .turned
            .is_none_or(|(h, t)| places::turn(h, heading).abs() > 5.0 || now - t > TURN_RETRY_SECS);
        if due {
            #[allow(clippy::cast_possible_truncation)]
            frame.nav(NavRequest::Face(heading as f32));
            r.turned = Some((heading, now));
        }
    }

    /// Steer toward the running goal; answers the movement if it ended this frame.
    pub fn step(&mut self, frame: &mut HookFrame<'_>) -> Option<Settled> {
        let now = frame.local_time;
        let letting_go = std::mem::take(&mut self.travel_letting_go);
        self.locked_doors.extend(frame.locked.iter().copied());
        // A key or the run lock let go of takes effect later in the frame, and the game's stop
        // ends a turn-to-heading: a turn is asked for the frame after.
        let mut stopping = letting_go || !self.unheld.is_empty();
        Self::let_go_of(frame, &mut self.unheld);
        if frame.player_input {
            self.chain = None;
            // The player has the controls: the run lock is theirs, and a movement ends with
            // whatever it held let go.
            self.player_moved = Some(now);
            self.run_owner = None;
            self.release = false;
            self.asked_run = None;
            Self::let_go_of(frame, &mut self.held);
            return self
                .running
                .take()
                .map(|r| settled(&r, Err("interrupted".into())));
        }
        if let Some((plugin, on)) = self.asked_run.take() {
            self.run(frame, &plugin, on);
        }
        if std::mem::take(&mut self.release) && self.run_owner.is_none() && frame.places.autorun() {
            frame.nav(NavRequest::Autorun(false));
            stopping = true;
        }
        if let Some(mut c) = self.chain.take() {
            c.frames = c.frames.saturating_sub(1);
            if c.frames == 0 || now > c.until {
                self.run(frame, &c.plugin, false);
                Self::let_go_of(frame, &mut self.held);
            } else {
                self.chain = Some(c);
            }
        }
        // Using a door may have the server turn the character to it: a move along opening one
        // carries on.
        let at_door = self.running.as_ref().is_some_and(|r| r.door.is_some());
        if frame.server_moving && !at_door {
            self.chain = None;
            // The server is moving the character: nothing of the host's may fight it.
            let owner = self.run_owner.clone();
            if let Some(p) = owner {
                self.run(frame, &p, false);
            }
            Self::let_go_of(frame, &mut self.held);
            return self
                .running
                .take()
                .map(|r| settled(&r, Err("interrupted".into())));
        }
        let mut r = self.running.take()?;
        let plugin = r.plugin.clone();
        let Some(me) = frame.places.player() else {
            return if now - r.started > 1.0 {
                self.run(frame, &plugin, false);
                Self::let_go_of(frame, &mut self.held);
                Some(settled(&r, Err("no_position".into())))
            } else {
                self.running = Some(r);
                None
            };
        };
        let end = match r.goal {
            // The travel lets go of the keys later in this frame.
            Goal::MoveAlong { .. } if letting_go => None,
            Goal::MoveAlong { range, .. } => self.along(&mut r, frame, &me, range, now),
            Goal::Face { heading } => {
                if places::turn(places::heading(&me), heading).abs() < 5.0 {
                    Some(Ok(Value::map().with("heading", heading)))
                } else if now - r.started > FACE_SECS {
                    Some(Err("timeout".to_owned()))
                } else {
                    if !stopping {
                        Self::face(&mut r, frame, heading, now);
                    }
                    None
                }
            }
            Goal::MoveTo { target, range } => {
                let at: Option<Position> = match target {
                    Anchor::Object(o) => frame.places.object(o),
                    Anchor::Place(p) => Some(p),
                };
                match at {
                    None => {
                        self.run(frame, &plugin, false);
                        Some(Err("target_gone".to_owned()))
                    }
                    Some(at) => {
                        let d = places::ground_distance(&me, &at);
                        let best = r.best.get_or_insert((d, now));
                        if d < best.0 - 0.5 {
                            *best = (d, now);
                        }
                        let (_, best_at) = *best;
                        let first = *r.from.get_or_insert(d);
                        if d <= range {
                            self.arrived(&r, now);
                            Some(Ok(Value::map().with("distance", (d * 10.0).round() / 10.0)))
                        } else if now - best_at > STUCK_SECS {
                            self.run(frame, &plugin, false);
                            Some(Err("stuck".to_owned()))
                        } else if now - r.started > first / SLOWEST_PACE + 20.0 {
                            self.run(frame, &plugin, false);
                            Some(Err("timeout".to_owned()))
                        } else {
                            if !stopping {
                                self.steer(&mut r, frame, &me, &at, range, now);
                            }
                            None
                        }
                    }
                }
            }
            Goal::Jump {
                heading,
                hold,
                walk,
                direction,
            } => match (r.pressed, r.leapt) {
                (None, _) => match heading {
                    Some(h) if places::turn(places::heading(&me), h).abs() >= 5.0 => {
                        if now - r.started > FACE_SECS {
                            Some(Err("timeout".to_owned()))
                        } else {
                            if !stopping {
                                Self::face(&mut r, frame, h, now);
                            }
                            None
                        }
                    }
                    _ => {
                        // The jump first, so a charge from a standstill takes the direction held
                        // after it as the way it leaps, without moving while it charges.
                        self.press(frame, NavKey::Jump);
                        if walk {
                            self.press(frame, NavKey::Walk);
                        }
                        if let Some(d) = direction {
                            self.press(frame, d);
                        }
                        r.pressed = Some(now);
                        None
                    }
                },
                (Some(at), None) => {
                    if now - at >= hold {
                        frame.nav(NavRequest::Hold(NavKey::Jump, false));
                        self.held.retain(|k| *k != NavKey::Jump);
                        r.leapt = Some(now);
                    }
                    None
                }
                (Some(at), Some(leapt)) => {
                    if now > leapt {
                        Self::let_go_of(frame, &mut self.held);
                        Some(Ok(Value::map()
                            .with("held", ((leapt - at) * 1000.0).round())
                            .with("heading", places::heading(&me).round())))
                    } else {
                        None
                    }
                }
            },
            Goal::Motion { key, secs, walk } => match r.pressed {
                None => {
                    if walk {
                        self.press(frame, NavKey::Walk);
                    }
                    self.press(frame, key);
                    r.pressed = Some(now);
                    None
                }
                Some(at) if now - at >= secs => {
                    Self::let_go_of(frame, &mut self.held);
                    Some(Ok(Value::map().with("held", ((now - at) * 1000.0).round())))
                }
                Some(_) => None,
            },
            Goal::Follow { object, range } => match frame.places.object(object) {
                None if now - r.seen > LOST_SECS => {
                    self.run(frame, &plugin, false);
                    Some(Err("lost".to_owned()))
                }
                None => {
                    self.run(frame, &plugin, false);
                    None
                }
                Some(at) => {
                    r.seen = now;
                    let d = places::ground_distance(&me, &at);
                    if d <= range {
                        self.run(frame, &plugin, false);
                    } else if !stopping
                        && (d > range + 1.0 || self.run_owner.as_deref() == Some(plugin.as_str()))
                    {
                        self.steer(&mut r, frame, &me, &at, range, now);
                    }
                    None
                }
            },
        };
        match end {
            Some(result) => Some(settled(&r, result)),
            None => {
                self.running = Some(r);
                None
            }
        }
    }

    /// `r` arrived: what it ran with stays held a moment, for its plugin's next movement.
    fn arrived(&mut self, r: &Running, now: f64) {
        if self.run_owner.as_deref() == Some(r.plugin.as_str()) || !self.held.is_empty() {
            self.chain = Some(Chain {
                plugin: r.plugin.clone(),
                frames: CHAIN_FRAMES,
                until: now + CHAIN_SECS,
                follower: r.follower.clone(),
            });
        }
    }

    /// One frame of a move along: the journeys' follower over the plugin's own points, with the
    /// forward and turn keys, and each point reached told. Answers its end, if it ended.
    #[allow(clippy::cast_possible_truncation)]
    fn along(
        &mut self,
        r: &mut Running,
        frame: &mut HookFrame<'_>,
        me: &Position,
        range: f64,
        now: f64,
    ) -> Option<Result<Value, String>> {
        let Goal::MoveAlong { points, .. } = &r.goal else {
            return None;
        };
        let follower = r.follower.get_or_insert_with(|| {
            let path = NavPath {
                waypoints: std::iter::once(me)
                    .chain(points)
                    .map(|p| Waypoint {
                        position: *p,
                        action: None,
                    })
                    .collect(),
                legs: Vec::new(),
                goal_offset: 0.0,
                cuts: Vec::new(),
            };
            let config = FollowConfig::strict(range as f32, STUCK_SECS);
            match r.before.take() {
                Some(before) => before.then(path, config),
                None => Follower::new(path, config),
            }
        });
        let next = follower.next_waypoint();
        let toward = follower.path().waypoints.get(next).map(|w| w.position);
        if let Some(end) = self.door_on_the_way(r, frame, me, toward.as_ref(), now) {
            return end;
        }
        let follower = r.follower.as_mut()?;
        let steer = follower.update(me, places::heading(me) as f32, now);
        for index in next..follower.next_waypoint() {
            self.progress.push((
                r.plugin.clone(),
                "nav.point_reached",
                Value::map()
                    .with("id", r.id)
                    .with("index", i64::try_from(index).unwrap_or(0)),
            ));
        }
        match steer {
            Steer::Arrived => {
                let (count, last) = match &r.goal {
                    Goal::MoveAlong { points, .. } => (points.len(), points.last().copied()),
                    _ => (0, None),
                };
                let d = last.map_or(0.0, |p| places::ground_distance(me, &p));
                self.set_turn(frame, None);
                self.arrived(r, now);
                Some(Ok(Value::map()
                    .with("distance", (d * 10.0).round() / 10.0)
                    .with("points", i64::try_from(count).unwrap_or(0))))
            }
            Steer::Go {
                forward,
                turn,
                face,
                ..
            } => {
                self.set_held(frame, NavKey::Forward, forward);
                self.set_turn(frame, turn);
                if let Some(heading) = face {
                    frame.nav(NavRequest::Heading(heading));
                }
                None
            }
            // The plugin's points carry no doors, so a door is never asked for.
            Steer::Stuck { .. } | Steer::OpenDoor(_) => {
                Self::let_go_of(frame, &mut self.held);
                Some(Err("stuck".to_owned()))
            }
        }
    }

    /// A shut door across a move along's way, near: the character stands, and the door is used,
    /// and used again after a while, as a journey's walk uses one. Answers the move's end when the
    /// door will not open (locked, the server says, or still shut after its uses), and that the
    /// frame is the door's (`Some(None)`) while it is opened; `None` when there is no such door,
    /// or it stands open.
    #[allow(clippy::cast_possible_truncation)]
    fn door_on_the_way(
        &mut self,
        r: &mut Running,
        frame: &mut HookFrame<'_>,
        me: &Position,
        toward: Option<&Position>,
        now: f64,
    ) -> Option<Option<Result<Value, String>>> {
        let open = |id: ObjectId| {
            frame
                .world
                .physics(id)
                .is_some_and(|p| p.state & ETHEREAL != 0)
        };
        let door = match r.door {
            Some((d, ..)) => d,
            None => {
                let to = toward?;
                let (ax, ay, _) = places::global(me);
                let (bx, by, _) = places::global(to);
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = (dx * dx + dy * dy).max(1e-6);
                frame
                    .world
                    .tables
                    .weenies
                    .iter()
                    .filter(|(id, w)| crate::reads::class_of(w).1 == "Door" && !open(*id))
                    .filter_map(|(id, _)| {
                        let at = frame.places.object(id)?;
                        let (x, y, _) = places::global(&at);
                        let t = ((x - ax) * dx + (y - ay) * dy) / len2;
                        let across = ((x - ax - t * dx).powi(2) + (y - ay - t * dy).powi(2)).sqrt();
                        let near = places::ground_distance(me, &at);
                        (t > 0.0 && across <= DOOR_ACROSS && near <= DOOR_NEAR)
                            .then_some((near, id))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))?
                    .1
            }
        };
        if open(door) {
            // Open: on through it, from standing.
            self.locked_doors.remove(&door);
            r.door = None;
            if let Some(f) = r.follower.as_mut() {
                f.held_up(now);
            }
            return None;
        }
        self.set_held(frame, NavKey::Forward, false);
        self.set_turn(frame, None);
        if let Some(f) = r.follower.as_mut() {
            f.held_up(now);
        }
        let shut = |why: &'static str| {
            let d = door_value(frame.world, door).with("why", why);
            let name = d.get("name").as_str().unwrap_or("the door").to_owned();
            (d, name)
        };
        if self.locked_doors.contains(&door) {
            let (d, name) = shut("locked");
            self.shut_doors.insert((r.plugin.clone(), r.id), d);
            Self::let_go_of(frame, &mut self.held);
            return Some(Some(Err(format!(
                "door_shut: {name} is locked, the server says"
            ))));
        }
        match r.door {
            Some((_, since, tries)) if now - since <= DOOR_WAIT => {
                r.door = Some((door, since, tries));
            }
            Some((_, _, tries)) if tries >= DOOR_TRIES => {
                let (d, name) = shut("no_answer");
                self.shut_doors.insert((r.plugin.clone(), r.id), d);
                Self::let_go_of(frame, &mut self.held);
                return Some(Some(Err(format!(
                    "door_shut: {name} did not open after {DOOR_TRIES} uses"
                ))));
            }
            used => {
                frame.request(dereth_client_contract::UiRequest::Use(door));
                r.door = Some((door, now, used.map_or(0, |(_, _, n)| n) + 1));
            }
        }
        Some(None)
    }

    /// Run toward `at` while it is ahead; when it is off to the side, stop and turn in place first.
    /// A turn is never asked for while running: the client's turn-to-heading ends forward motion
    /// when it finishes. Close by, it runs on past a target it will pass within `range` of.
    fn steer(
        &mut self,
        r: &mut Running,
        frame: &mut HookFrame<'_>,
        me: &Position,
        at: &Position,
        range: f64,
        now: f64,
    ) {
        let b = places::bearing(me, at);
        let off = places::turn(places::heading(me), b).abs();
        let plugin = r.plugin.clone();
        let running = self.run_owner.as_deref() == Some(plugin.as_str()) && frame.places.autorun();
        // How near it passes, running on as it faces.
        let passes =
            places::ground_distance(me, at) * dereth_primitives::num::math::sin(off.to_radians());
        if off > TURN_TOLERANCE && !(running && off < 90.0 && passes <= PASS_WITHIN * range) {
            self.run(frame, &plugin, false);
            // Running, it stops this frame and turns from the next: the stop would end the turn.
            if !running {
                Self::face(r, frame, b, now);
            }
        } else {
            self.run(frame, &plugin, true);
        }
    }
}

/// A journey's leg as plugins read it: `{ kind, to, ... }` by kind.
#[must_use]
pub fn leg_value(leg: &TravelLeg) -> Value {
    let v = Value::map().with("kind", leg.kind());
    let place = |p: &Position| places::to_value(p);
    match leg {
        TravelLeg::Walk { to, path } => v
            .with("to", place(to))
            .with("path", Value::List(path.iter().map(place).collect())),
        TravelLeg::Door { object, at } => v.with("object", object.0).with("to", place(at)),
        TravelLeg::Portal {
            name,
            wcid,
            from,
            to,
            source,
        } => {
            let mut v = v
                .with("name", name.as_str())
                .with("from", place(from))
                .with("source", source.word());
            if let Some(w) = wcid {
                v = v.with("class", *w);
            }
            if let Some(t) = to {
                v = v.with("to", place(t));
            }
            v
        }
        TravelLeg::Recall {
            name,
            recall,
            item,
            to,
        } => {
            let mut v = v
                .with("name", name.as_str())
                .with("recall", recall.word())
                .with("to", place(to));
            if let Some(i) = item {
                v = v.with("item", i.0);
            }
            v
        }
        TravelLeg::HandBack {
            name,
            token,
            at,
            to,
        } => v
            .with("name", name.as_str())
            .with("token", token.as_str())
            .with("from", place(at))
            .with("to", place(to)),
        TravelLeg::Drop { from, to, height } => v
            .with("from", place(from))
            .with("to", place(to))
            .with("height", f64::from(*height)),
    }
}

/// A planned journey as plugins read it: `{ legs, seconds }`.
#[must_use]
pub fn plan_value(plan: &TravelPlan) -> Value {
    Value::map()
        .with(
            "legs",
            Value::List(plan.legs.iter().map(leg_value).collect()),
        )
        .with("seconds", (f64::from(plan.seconds) * 10.0).round() / 10.0)
}

/// A walking route as plugins read it: `{ points, doors, length, complete }`, each door
/// `{ object, at }` with `at` the corner (counting from 1) it is opened from.
#[must_use]
pub fn route_value(route: &TravelRoute) -> Value {
    Value::map()
        .with(
            "points",
            Value::List(route.points.iter().map(places::to_value).collect()),
        )
        .with(
            "doors",
            Value::List(
                route
                    .doors
                    .iter()
                    .map(|(object, at)| {
                        Value::map()
                            .with("object", object.0)
                            .with("at", i64::try_from(*at).unwrap_or(0) + 1)
                    })
                    .collect(),
            ),
        )
        .with("length", (f64::from(route.length) * 10.0).round() / 10.0)
        .with("complete", route.complete)
}

/// What a use taught, as plugins read it.
fn learned_value(what: &Learned) -> Value {
    match what {
        Learned::Portal { name, to } => Value::map()
            .with("kind", "portal")
            .with("name", name.as_str())
            .with("to", places::to_value(to)),
        Learned::Refusal { name, reason } => Value::map()
            .with("kind", "refusal")
            .with("name", name.as_str())
            .with("reason", reason.as_str()),
        Learned::Recall { recall, to } => Value::map()
            .with("kind", "recall")
            .with("recall", recall.word())
            .with("to", places::to_value(to)),
    }
}
