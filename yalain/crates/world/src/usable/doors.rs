//! Doors: a player walks to a door and uses it; it opens for everyone who sees it, stays open ten
//! seconds and closes by itself.
//!
//! **Opening.** The player walks to within two meters. The door turns passable and plays its
//! opening; when the opening has played it is open and the use is answered. Ten seconds later it
//! turns solid again, plays its closing, and is closed when that has played. Doors differ only in
//! how long these take.
//!
//! **Using it again.** A door used while it is open closes at once, the use answered when it is
//! closed; one used while closing is shut at once, the use answered when its closing would have
//! ended. A use while it opens is refused as the item busy, and for three seconds after a use
//! closed it a use is refused as too soon.
//!
//! **Locks.** A locked door is locked from its front only: from behind it opens to anyone. From the
//! front it refuses a plain use (a player beyond reach is not walked there). The key whose id is the
//! door's is used on it from within three meters: the player turns it, the door unlocks and opens,
//! and as it is open the key is used up and the use is answered "Your key fits the lock."; another
//! key is refused as the wrong one. The door locks again as it closes.
//!
//! **Creatures.** A creature or pet on its way to a place beyond a closed door comes to about a
//! meter and a half from it and opens it as a player's use does (the same messages, the same ten
//! seconds open), then stands until it is open and goes on through. One whose template says it
//! does not use doors (the summoned creatures) cannot: it stands at a closed door until someone
//! else opens it. A lock holds a creature as it holds a player, from the door's front, except at
//! the doors whose kind or template lets creatures through their locks.

use std::collections::{HashMap, HashSet};

use auberean_protocol::physics::{ObjectEvent, Word};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use dereth_primitives::num::math;
use yalain_net::ConnId;
use yalain_net::ServerNet;
use yalain_store::{Store, StoredPosition};

use crate::places::{self, Use};
use crate::{presence, progress, Clock, Server, Timer};

/// How close a player walks to a door to use it, and to use a key on it, in meters (2017: 148
/// walks to a door at 2.0 m; 6 of 8 walks with a key at 3.0 m).
pub const REACH: f32 = 2.0;
pub const KEY_REACH: f32 = 3.0;

/// How long a door stays open once it has opened, in seconds (2017: 10.008 s median, 635 closings).
pub const STAYS_OPEN: f64 = 10.0;

/// How long after a use that closed a door a use is refused as too soon, in seconds (2017: refused
/// up to 2.77 s after, made from 3.15 s).
pub const LOCKOUT: f64 = 3.0;

/// How long a key turns in the lock before the door moves, in seconds (2017: 1.39 s median from
/// the turning behavior to the door's use, 24 of 26 within 1.29-1.47 s), and the behavior.
pub const KEY_TURNING: f64 = 1.39;
pub const TURN_KEY: u32 = 0x4100_00FA;

/// The key ids the vault keys' kind gives them (their templates name none): Ariaki's to
/// Winterhollows', in their templates' order.
pub const VAULT_KEYS: (u32, u32, u32) = (0x4700_1126, 0x4700_1137, 0x4000_0032);

/// The door's qualities its opening and closing set: how solid it is (passable while open) and
/// whether it is open.
pub mod quality {
    pub const PHYSICS: u32 = 3;
    pub const PASSABLE: i32 = 0x0020_0020;
    pub const SOLID: i32 = 0;
    pub const OPEN: u32 = 702;
}

/// The modes a door plays.
pub mod mode {
    pub const OPEN: u32 = 0x4100_0002;
    pub const CLOSED: u32 = 0x4100_0003;
}

/// Why a use of a door is refused, as its answer says.
pub mod refusal {
    /// The door is locked.
    pub const LOCKED: u32 = 10_030;
    /// A use came too soon after one closed it.
    pub const TOO_SOON: u32 = 10_007;
    /// The door is opening.
    pub const BUSY: u32 = 1039;
    /// The key is not the door's.
    pub const WRONG_KEY: u32 = 10_017;
}

/// The control flags of each refusal's answer (2017: locked 956, too soon 828, busy 804).
#[must_use]
pub const fn refusal_flags(why: u32) -> Option<u32> {
    match why {
        refusal::LOCKED => Some(USE_FLAGS),
        refusal::TOO_SOON => Some(828),
        refusal::BUSY => Some(804),
        _ => None,
    }
}

/// The script package every door derives from.
pub const PACKAGE: &str = "Door";

/// How near a closed door's plane a creature on its way through it comes before it opens it, in
/// meters (2017: 81 creatures and pets opening a door stood a median 1.53 m from its plane, p10
/// 0.98, p90 2.51, and 1.59 m from its middle).
pub const CREATURE_STOPS: f32 = 1.5;

/// Does a door of this script package let creatures through its lock (its script says so)?
#[must_use]
pub fn package_lets_creatures_through(name: &str) -> bool {
    matches!(
        name,
        "DoorEmperianHaunted" | "DoorEmperianMossy" | "DoorHuman"
    )
}

/// The control flags of a door's answer.
pub const USE_FLAGS: u32 = 956;

/// The answer to a key that opened its door: "Your %1 fits the lock.", with the door's name and
/// the key's.
pub mod text {
    pub const FITS: (u32, u32) = (0x2317_692F, 0x2500_0014);
    /// The locked object's name (a door's or a chest's), and the key's.
    pub const DOOR: u32 = 0x1C06_9478;
    pub const KEY: u32 = 0x9352_2CD0;
}

/// The kinds of door, by how long their opening and closing take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    /// A mine's door closes quickly.
    Mine,
    /// A Virindi door opens and closes slowly.
    Virindi,
}

impl Kind {
    /// How long its opening takes, in seconds (2017: 1.27 s median over 693 openings; Virindi
    /// doors 2.26 s, 24).
    #[must_use]
    pub const fn opening(self) -> f64 {
        match self {
            Self::Virindi => 2.26,
            Self::Plain | Self::Mine => 1.27,
        }
    }

    /// How long its closing takes, in seconds (2017: 2.47 s median; mine doors 1.0 s, 16; Virindi
    /// doors 2.29 s, 18).
    #[must_use]
    pub const fn closing(self) -> f64 {
        match self {
            Self::Mine => 1.0,
            Self::Virindi => 2.29,
            Self::Plain => 2.47,
        }
    }

    /// The kind a door's script package's name makes it.
    #[must_use]
    pub fn of_package(name: &str) -> Self {
        match name {
            "DoorMine" => Self::Mine,
            "DoorVirindi" => Self::Virindi,
            _ => Self::Plain,
        }
    }
}

/// Where a door is in its moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

/// A use of a door waiting for it to finish moving: the player, its use, how far it stood, and
/// the key it used (its id), if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub world: ConnId,
    pub used: Use,
    pub distance: f32,
    pub key: Option<u64>,
}

/// One door's state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Door {
    pub phase: Phase,
    /// Counts the door's movements, so a step of an earlier one does nothing.
    pub generation: u32,
    /// When a use last closed it (local seconds).
    pub closed_by_use: Option<f64>,
    /// Its key has unlocked it (until it closes).
    pub unlocked: bool,
    pub waiting: Option<Waiting>,
    /// When its last closing began (local seconds).
    pub closing_since: f64,
    /// A use that shut it while it closed, answered when that closing would have ended.
    pub cut_short: Option<Waiting>,
}

/// The doors' states, the kinds of door by their script packages, and the packages whose doors
/// let creatures through their locks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Doors {
    pub by_id: HashMap<u64, Door>,
    pub packages: HashMap<u32, Kind>,
    pub creatures_through_locks: HashSet<u32>,
}

/// A step of a door's moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A key has turned: the door begins to open.
    Turned,
    /// The opening has played: it is open.
    Opened,
    /// It has stood open long enough: it begins to close.
    Close,
    /// The closing has played: it is closed.
    Closed,
    /// The closing a use cut short would have ended: the use is answered.
    CutShortEnds,
}

/// A door of the world, by its id: its kind and where it stands, when the id is a door's (an
/// object with no destination whose script package is a door's).
#[must_use]
pub fn door<S: Store>(s: &Server<S>, id: u64) -> Option<(Kind, StoredPosition)> {
    let i = presence::content_by_id(&s.content.objects, id)?;
    let o = &s.content.objects[i];
    if o.destination.is_some() {
        return None;
    }
    let kind = *s.doors.packages.get(&o.description.package)?;
    Some((
        kind,
        StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        },
    ))
}

/// Is a place in front of a door (on the side its facing points to)?
#[must_use]
pub fn in_front(door: &StoredPosition, at: &StoredPosition) -> bool {
    let [w, _, _, z] = door.rotation;
    let heading = 2.0 * math::atan2f(z, w);
    let (dx, dy) = crate::usable::maps::offset(door, at);
    -math::sinf(heading) * dx + math::cosf(heading) * dy > 0.0
}

/// The key id a key's template gives it.
#[must_use]
pub fn key_id<S: Store>(s: &Server<S>, template: u32) -> Option<u32> {
    let (first, last, id) = VAULT_KEYS;
    if (first..=last).contains(&template) {
        return Some(id + (template - first));
    }
    s.catalog
        .get(template)
        .and_then(|t| t.props.word("Lock_KeyID"))
}

/// Is a player's use waiting on a door?
#[must_use]
pub fn waiting_on<S: Store>(s: &Server<S>, world: ConnId) -> bool {
    s.doors.by_id.values().any(|d| {
        [&d.waiting, &d.cut_short]
            .into_iter()
            .any(|w| w.as_ref().is_some_and(|w| w.world == world))
    })
}

/// Is a door locked to a player standing at a place: locked, not unlocked by its key since it last
/// closed, and the player in front of it?
fn locked<S: Store>(s: &Server<S>, id: u64, at: &StoredPosition) -> bool {
    s.locks.of(id).is_some()
        && !s.doors.by_id.get(&id).is_some_and(|d| d.unlocked)
        && door(s, id).is_some_and(|(_, d)| in_front(&d, at))
}

fn position<S: Store>(s: &Server<S>, world: ConnId) -> Option<StoredPosition> {
    s.players.get(&world).map(|p| p.position)
}

fn me<S: Store>(s: &Server<S>, world: ConnId) -> u64 {
    s.players.get(&world).map_or(0, |p| p.character)
}

/// Answer a use of a door with a refusal: no effective target, each refusal's flags.
fn refuse<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId, u: &Use, why: u32) {
    let distance = door(s, walk_goal(u))
        .and_then(|(_, d)| places::reach(&u.from, d.cell, d.origin))
        .unwrap_or(0.0);
    let m = places::use_refused(u, 0, refusal_flags(why), distance, why);
    progress::private(net, world, &m);
}

/// The door a use is about: the object a key is used on, else the object used.
fn walk_goal(u: &Use) -> u64 {
    if u.target == 0 {
        u.item
    } else {
        u.target
    }
}

/// Refused before any walk: a door opening is busy, and one a use just closed refuses for a while.
fn refused_at_once<S: Store>(s: &Server<S>, id: u64, now: f64) -> Option<u32> {
    let d = s.doors.by_id.get(&id)?;
    if d.phase == Phase::Opening {
        Some(refusal::BUSY)
    } else if d.closed_by_use.is_some_and(|t| now - t < LOCKOUT) {
        Some(refusal::TOO_SOON)
    } else {
        None
    }
}

/// A player asks to use a door: refused at once, or walked to it, or used where it stands.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    let Some((_, d)) = door(s, u.item) else {
        return;
    };
    if let Some(why) = refused_at_once(s, u.item, clock.local.0) {
        refuse(s, net, world, &u, why);
        return;
    }
    let Some(here) = position(s, world) else {
        return;
    };
    let closed = s
        .doors
        .by_id
        .get(&u.item)
        .is_none_or(|x| x.phase == Phase::Closed);
    let in_reach = places::reach(&here, d.cell, d.origin).is_some_and(|x| x <= REACH);
    if closed && locked(s, u.item, &here) {
        // Refused where the player stands: a walk begun is stopped at once.
        if !in_reach {
            places::walk_to(s, net, world, u.clone(), REACH);
            places::stop_walking(s, net, world);
        }
        refuse(s, net, world, &u, refusal::LOCKED);
    } else if in_reach {
        at_door(s, net, world, u, clock);
    } else {
        places::walk_to(s, net, world, u, REACH);
        places::arrived(s, net, world, clock);
    }
}

/// The player is at the door: refused, or the door opens, or closes.
pub fn at_door<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let id = u.item;
    let Some((kind, d)) = door(s, id) else {
        return;
    };
    let now = clock.local.0;
    if let Some(why) = refused_at_once(s, id, now) {
        refuse(s, net, world, &u, why);
        return;
    }
    let here = position(s, world).unwrap_or(u.from);
    let distance = places::reach(&u.from, d.cell, d.origin).unwrap_or(0.0);
    let phase = s.doors.by_id.get(&id).map(|d| d.phase).unwrap_or_default();
    match phase {
        Phase::Closed if locked(s, id, &here) => refuse(s, net, world, &u, refusal::LOCKED),
        Phase::Closed | Phase::Opening => {
            let waiting = Waiting {
                world,
                used: u,
                distance,
                key: None,
            };
            open(s, net, id, kind, Some(waiting), clock);
        }
        Phase::Open => {
            let d = s.doors.by_id.entry(id).or_default();
            d.closed_by_use = Some(now);
            d.waiting = Some(Waiting {
                world,
                used: u,
                distance,
                key: None,
            });
            close(s, net, id, kind, clock);
        }
        Phase::Closing => {
            // The closing is cut short: shut at once (it is solid already), the use answered when
            // the closing would have ended.
            let d = s.doors.by_id.entry(id).or_default();
            d.closed_by_use = Some(now);
            d.phase = Phase::Closed;
            d.unlocked = false;
            d.generation = d.generation.wrapping_add(1);
            d.waiting = None;
            let left = (d.closing_since + kind.closing() - now).max(0.0);
            d.cut_short = Some(Waiting {
                world,
                used: u,
                distance,
                key: None,
            });
            let generation = d.generation;
            let m = ServerMessage::DoMode(ObjectEvent {
                sender: places::object_stamp(s, id),
                action: Word(mode::CLOSED),
            });
            places::to_knowers(s, net, id, &m);
            shown_open(s, net, id, false);
            s.after(
                clock,
                left,
                Timer::Door {
                    door: id,
                    generation,
                    step: Step::CutShortEnds,
                },
            );
        }
    }
}

/// A player uses a key it holds on a door: walked to within three meters, then the key turns.
pub fn key_used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let Some((_, d)) = door(s, u.target) else {
        return;
    };
    let Some(here) = position(s, world) else {
        return;
    };
    if places::reach(&here, d.cell, d.origin).is_some_and(|x| x <= KEY_REACH) {
        key_at_door(s, net, world, u, clock);
    } else {
        places::walk_to(s, net, world, u, KEY_REACH);
        places::arrived(s, net, world, clock);
    }
}

/// The player is at the door with its key: a door not locked to it is used as without the key;
/// a key whose id is not the door's is refused as the wrong key; the door's key turns (the player
/// plays turning it), and a moment later the door unlocks and opens.
pub fn key_at_door<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let id = u.target;
    let Some((_, d)) = door(s, id) else {
        return;
    };
    let here = position(s, world).unwrap_or(u.from);
    let phase = s.doors.by_id.get(&id).map(|d| d.phase).unwrap_or_default();
    if phase != Phase::Closed || !locked(s, id, &here) {
        let mut plain = u;
        plain.item = id;
        plain.target = 0;
        at_door(s, net, world, plain, clock);
        return;
    }
    let key = s.things.get(u.item).map(|t| (t.id, t.template));
    let wanted = s.locks.of(id).and_then(|l| l.key);
    let fits = key.is_some_and(|(_, template)| wanted.is_some() && key_id(s, template) == wanted);
    if !fits {
        refuse(s, net, world, &u, refusal::WRONG_KEY);
        return;
    }
    let distance = places::reach(&u.from, d.cell, d.origin).unwrap_or(0.0);
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    if let Some(p) = s.players.get_mut(&world) {
        let turns = ServerMessage::PlayBehavior(crate::combat::player_event(
            p,
            BehaviorParams {
                bits: BehaviorParams::BEHAVIOR | BehaviorParams::CONTEXT,
                behavior: TURN_KEY,
                context,
                ..BehaviorParams::default()
            },
        ));
        let me = p.character;
        crate::combat::to_all_seeing(s, net, me, &turns);
    }
    let d = s.doors.by_id.entry(id).or_default();
    d.phase = Phase::Opening;
    d.generation = d.generation.wrapping_add(1);
    let generation = d.generation;
    d.waiting = Some(Waiting {
        world,
        used: u,
        distance,
        key: key.map(|(k, _)| k),
    });
    s.after(
        clock,
        KEY_TURNING,
        Timer::Door {
            door: id,
            generation,
            step: Step::Turned,
        },
    );
}

/// A door as it is created for a player that comes to see it: closed, or passable and in its open
/// mode while it is open.
#[must_use]
pub fn created<S: Store>(
    s: &Server<S>,
    id: u64,
    o: &yalain_content::ContentObject,
) -> auberean_protocol::physics::CreateObject {
    let mut m = crate::player::content_object(id, o);
    // An object placed again by a reload is made with its newer instance stamp.
    m.physics.instance_stamp = s.placed.instance(id);
    // A person walking its route is made with the stamp its place was last told with.
    if let Some(stamp) = crate::walkers::position_stamp(s, id) {
        m.physics.stamps[0] = stamp;
    }
    if door(s, id).is_some() {
        let open = s
            .doors
            .by_id
            .get(&id)
            .is_some_and(|d| matches!(d.phase, Phase::Opening | Phase::Open));
        m.physics.mode = if open { mode::OPEN } else { mode::CLOSED };
        if open {
            m.weenie.numbers[crate::player::NUMBERS_FROM] = quality::PASSABLE;
        }
    }
    m
}

/// What the doors on a creature's way do to its next step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Passage {
    /// No closed door is near on its way: it steps as it would.
    Clear,
    /// It comes to a closed door: it steps no farther than this, in meters.
    UpTo(f32),
    /// It stands at a door that is opening or closing.
    Waits,
    /// A closed door it cannot open stands in its way.
    Shut,
}

/// A door near a creature's way: the door, its kind, and how far along the way its plane is.
struct OnTheWay {
    id: u64,
    kind: Kind,
    along: f32,
}

/// A place in a door's frame: along the door, and out of its front.
fn in_frame(door: &StoredPosition, at: &StoredPosition) -> (f32, f32) {
    let [w, _, _, z] = door.rotation;
    let heading = 2.0 * math::atan2f(z, w);
    let (dx, dy) = crate::usable::maps::offset(door, at);
    let (sin, cos) = (math::sinf(heading), math::cosf(heading));
    (cos * dx + sin * dy, -sin * dx + cos * dy)
}

/// The nearest door whose plane the way from `at` toward `aim` crosses within reach of its
/// middle, no farther along than `within`.
fn on_the_way<S: Store>(
    s: &Server<S>,
    at: &StoredPosition,
    aim: &StoredPosition,
    within: f32,
) -> Option<OnTheWay> {
    let mut best: Option<OnTheWay> = None;
    for (i, o) in s.content.objects.iter().enumerate() {
        if o.cell >> 16 != at.cell >> 16 || o.destination.is_some() {
            continue;
        }
        let Some(&kind) = s.doors.packages.get(&o.description.package) else {
            continue;
        };
        if (o.origin[2] - at.origin[2]).abs() > 3.0 {
            continue;
        }
        let door = StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        };
        let (ax, ay) = in_frame(&door, at);
        let (bx, by) = in_frame(&door, aim);
        // In the doorway already, or not going through.
        if ay.abs() < 0.05 || (ay > 0.0) == (by > 0.0) {
            continue;
        }
        let share = ay / (ay - by);
        if (ax + share * (bx - ax)).abs() > REACH {
            continue;
        }
        let along = share * math::hypotf(bx - ax, by - ay);
        if along <= within && best.as_ref().is_none_or(|b| along < b.along) {
            best = Some(OnTheWay {
                id: presence::content_id(&s.content.objects, i),
                kind,
                along,
            });
        }
    }
    best
}

/// Does a door let creatures through its lock: its template says so, or else its kind's script?
fn lets_creatures_through<S: Store>(s: &Server<S>, id: u64) -> bool {
    let Some(o) = presence::content_by_id(&s.content.objects, id).map(|i| &s.content.objects[i])
    else {
        return false;
    };
    match s
        .catalog
        .get(o.template)
        .and_then(|t| t.props.get("Lock_AICanIgnoreLock"))
    {
        Some(yalain_content::Prop::Bool(b)) => *b,
        _ => s
            .doors
            .creatures_through_locks
            .contains(&o.description.package),
    }
}

/// A creature at `at` steps up to `go` meters toward `aim`: what the doors on its way do. A closed
/// door ahead holds it [`CREATURE_STOPS`] from its plane; there it opens the door if it uses doors
/// and no lock holds it (the door's opening is everyone's to see), and stands until the door is
/// open; an open door lets it through.
pub fn creature_passage<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    at: &StoredPosition,
    aim: &StoredPosition,
    go: f32,
    clock: Clock,
) -> Passage {
    if s.doors.packages.is_empty() {
        return Passage::Clear;
    }
    let Some(door) = on_the_way(s, at, aim, go + CREATURE_STOPS + 0.01) else {
        return Passage::Clear;
    };
    let phase = s
        .doors
        .by_id
        .get(&door.id)
        .map(|d| d.phase)
        .unwrap_or_default();
    if phase == Phase::Open {
        return Passage::Clear;
    }
    if door.along > CREATURE_STOPS + 0.01 {
        return Passage::UpTo(door.along - CREATURE_STOPS);
    }
    if phase != Phase::Closed {
        return Passage::Waits;
    }
    let uses_doors = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.stats.movement.uses_doors);
    let held = locked(s, door.id, at) && !lets_creatures_through(s, door.id);
    if !uses_doors || held {
        return Passage::Shut;
    }
    if refused_at_once(s, door.id, clock.local.0).is_none() {
        open(s, net, door.id, door.kind, None, clock);
    }
    Passage::Waits
}

/// Is a use still good to go on: no key in it, or the key still held by who turned it?
pub(crate) fn key_kept<S: Store>(s: &Server<S>, world: ConnId, key: Option<u64>) -> bool {
    key.is_none_or(|key| {
        s.players
            .get(&world)
            .is_some_and(|p| p.play.items.iter().any(|h| h.id == key))
    })
}

/// Tell everyone who sees a door it turns passable or solid and plays a mode.
fn moving<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, physics: i32, m: u32) {
    let q = ServerMessage::UpdateIntVisual(VisualQuality {
        object: places::object_stamp(s, id),
        quality: quality::PHYSICS,
        value: physics,
    });
    places::to_knowers(s, net, id, &q);
    let m = ServerMessage::DoMode(ObjectEvent {
        sender: places::object_stamp(s, id),
        action: Word(m),
    });
    places::to_knowers(s, net, id, &m);
}

/// Tell everyone who sees a door whether it is open.
fn shown_open<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, open: bool) {
    let m = ServerMessage::UpdateBoolVisual(VisualQuality {
        object: places::object_stamp(s, id),
        quality: quality::OPEN,
        value: i32::from(open),
    });
    places::to_knowers(s, net, id, &m);
}

/// The door begins to open (a use waiting for it to be open, unless a creature opens it).
fn open<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    kind: Kind,
    waiting: Option<Waiting>,
    clock: Clock,
) {
    let d = s.doors.by_id.entry(id).or_default();
    d.phase = Phase::Opening;
    d.generation = d.generation.wrapping_add(1);
    d.waiting = waiting;
    let generation = d.generation;
    moving(s, net, id, quality::PASSABLE, mode::OPEN);
    s.after(
        clock,
        kind.opening(),
        Timer::Door {
            door: id,
            generation,
            step: Step::Opened,
        },
    );
}

/// The door begins to close.
fn close<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, kind: Kind, clock: Clock) {
    let d = s.doors.by_id.entry(id).or_default();
    d.phase = Phase::Closing;
    d.generation = d.generation.wrapping_add(1);
    d.closing_since = clock.local.0;
    let generation = d.generation;
    moving(s, net, id, quality::SOLID, mode::CLOSED);
    s.after(
        clock,
        kind.closing(),
        Timer::Door {
            door: id,
            generation,
            step: Step::Closed,
        },
    );
}

/// Answer the use waiting on a door: a key's one used up and "Your key fits the lock." (with the
/// flags of an item used on an object, no effective target), else the plain answer.
fn answer_waiting<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, w: Waiting) {
    let Some(key) = w.key else {
        let done = places::use_done(&w.used, me(s, w.world), USE_FLAGS, false, w.distance);
        progress::private(net, w.world, &done);
        return;
    };
    let door_name = presence::content_by_id(&s.content.objects, id)
        .and_then(|i| s.content.objects[i].description.name);
    let used = fits_answer(s, &w.used, key, door_name);
    crate::craft::use_up(s, net, w.world, key, 1);
    let done = places::use_done(&used, 0, places::USE_FLAGS_UNLOCKED, false, w.distance);
    progress::private(net, w.world, &done);
}

/// A key's use with its answer's text: "Your %1 fits the lock.", with the locked object's name and
/// the key's.
pub(crate) fn fits_answer<S: Store>(
    s: &Server<S>,
    u: &Use,
    key: u64,
    locked_name: Option<(u32, u32)>,
) -> Use {
    let key_name = s.things.get(key).and_then(|t| t.facts.name);
    // Its text can't be written without the key's name.
    if key_name.is_none() {
        return u.clone();
    }
    let vars = [
        locked_name.map(|n| progress::nested(text::DOOR, progress::text(n))),
        key_name.map(|n| progress::nested(text::KEY, progress::text(n))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut used = u.clone();
    used.message = Some(progress::text_with(text::FITS, vars));
    used
}

/// A door the world database's reload takes away or places again is let go: a use waiting on it is
/// refused as busy (a key used on it is kept), and it is closed, with no steps of its moving to come.
pub(crate) fn release<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(d) = s.doors.by_id.remove(&id) else {
        return;
    };
    for w in [d.waiting, d.cut_short].into_iter().flatten() {
        let busy = places::use_refused(&w.used, 0, None, w.distance, places::ACTIONS_LOCKED);
        progress::private(net, w.world, &busy);
    }
}

/// A step of a door's moving is due (none if the door has moved since).
pub fn step<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    generation: u32,
    step: Step,
    clock: Clock,
) {
    let Some((kind, _)) = door(s, id) else {
        return;
    };
    if step == Step::CutShortEnds {
        // Answered whatever the door has done since.
        if let Some(w) = s.doors.by_id.get_mut(&id).and_then(|d| d.cut_short.take()) {
            answer_waiting(s, net, id, w);
        }
        return;
    }
    if s.doors.by_id.get(&id).map(|d| d.generation) != Some(generation) {
        return;
    }
    match step {
        Step::Turned => {
            if let Some(w) = s.doors.by_id.get_mut(&id).and_then(|d| d.waiting.take()) {
                if key_kept(s, w.world, w.key) {
                    open(s, net, id, kind, Some(w), clock);
                } else {
                    if let Some(d) = s.doors.by_id.get_mut(&id) {
                        d.phase = Phase::Closed;
                    }
                    refuse(s, net, w.world, &w.used, refusal::WRONG_KEY);
                }
            }
        }
        Step::Opened => {
            let waiting = s.doors.by_id.get_mut(&id).and_then(|d| {
                d.phase = Phase::Open;
                d.waiting.take()
            });
            if let Some(w) = waiting.as_ref().filter(|w| !key_kept(s, w.world, w.key)) {
                refuse(s, net, w.world, &w.used, refusal::WRONG_KEY);
                close(s, net, id, kind, clock);
                return;
            }
            if waiting.as_ref().is_some_and(|w| w.key.is_some()) {
                if let Some(d) = s.doors.by_id.get_mut(&id) {
                    d.unlocked = true;
                }
            }
            // A key's answer comes with the door shown open; a plain use's just after it.
            match waiting {
                Some(w) if w.key.is_some() => {
                    answer_waiting(s, net, id, w);
                    shown_open(s, net, id, true);
                }
                Some(w) => {
                    shown_open(s, net, id, true);
                    answer_waiting(s, net, id, w);
                }
                None => shown_open(s, net, id, true),
            }
            s.after(
                clock,
                STAYS_OPEN,
                Timer::Door {
                    door: id,
                    generation,
                    step: Step::Close,
                },
            );
        }
        Step::Close => close(s, net, id, kind, clock),
        Step::CutShortEnds => {}
        Step::Closed => {
            // It locks again as it closes.
            let waiting = s.doors.by_id.get_mut(&id).and_then(|d| {
                d.phase = Phase::Closed;
                d.unlocked = false;
                d.waiting.take()
            });
            shown_open(s, net, id, false);
            if let Some(w) = waiting {
                answer_waiting(s, net, id, w);
            }
        }
    }
}
