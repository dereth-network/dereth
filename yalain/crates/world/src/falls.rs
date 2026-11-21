//! Falls: a player hurt by how far it falls, as its client reports its place.
//!
//! **A fall.** The places a player's client reports that go down make a descent, from the place
//! before the first that went down (its top). A descent is a fall once one of its steps goes down
//! at [`FALLING`] meters a second or faster and [`STEEP`] times as fast as it goes across, steeper
//! than any slope a body walks or rides down; each step is timed by the times the client reports
//! with its places. A fall's top is no higher than a body dropping from rest would have been to
//! reach that step's speed, so a walk down a hill before a drop is no part of it. A fall lands at
//! the first report that goes down no further, or that goes down at less than half its fastest step
//! (it met the ground and slides on); its height is from its top to the lowest place reported. A place the server put the player at (an entry, a trip's end, a
//! refusal), a gap of more than [`GAP`] seconds between reports, and a player dead or in portal
//! space all start afresh.
//!
//! **Harm** (2017's, fitted over the falls the 2017 server saw, harm by height): a fall of `h`
//! meters takes ⌊(h − 2.5) × (1 + 0.00123 × (h − 12.5)²)⌋ points of health, the last factor 1 below
//! 12.5 m, and `h` at most [`TOP_SPEED_HEIGHT`], where a fall reaches its greatest speed: nothing
//! below 3.5 m, 7 at 10 m, 18 at 20 m, 72 at 40 m, 1,015 at 100 m ([`harm`]). A mounted player takes
//! the first factor alone. The player's fall modifier multiplies it (1 unless set). A landing in
//! water, below its block's water surface, harms nothing (2017: a fall of 238 m into the sea cost
//! nothing).
//!
//! **Safe Fall.** A player holding the effect of Safe Fall (its type [`SAFE_FALL`]) lands unhurt
//! from a fall that would harm it, and the record ends with the fall it saved.
//!
//! **Told** as the player's health, the wounded look as any harm shows it, then "You suffer N impact
//! damage!" with the health it lost, sent in that order (2017: the line the next private message
//! after the health in 2,857 of 2,857 falls that told both in one tick; the look, or its stop, sent
//! before the line in 1,101 of 1,101 with one). With no health left the player dies, by misadventure,
//! and is told no line (2017: none at 4 of 4 deaths by misadventure).

use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::{presence, quals, Clock, Server};

/// How fast a step goes down, in meters a second, for its descent to be a fall.
pub const FALLING: f32 = 6.0;
/// How many times as fast as it goes across a step goes down for its descent to be a fall.
pub const STEEP: f32 = 1.2;
/// How fast a falling body gains speed, in meters a second each second.
pub const GRAVITY: f32 = 9.8;
/// How far a report goes down, in meters, to count as going down.
pub const DOWN: f32 = 0.05;
/// The longest gap between two reports, in seconds, that a descent goes on across.
pub const GAP: f64 = 2.0;
/// The height at which a fall reaches its greatest speed, in meters (2017: falls level off at
/// 50.5 m/s).
pub const TOP_SPEED_HEIGHT: f32 = 127.5;
/// The effect type of Safe Fall.
pub const SAFE_FALL: u32 = 552;
/// The player's fall modifier (a float quality).
pub const FALL_MODIFIER: u32 = 266;

/// "You suffer %1 impact damage!", its variable the health lost, and the kind of text it is shown as.
pub const IMPACT: (u32, u32) = (0x3C4C_6F74, 0x2500_000B);
pub const IMPACT_POINTS: u32 = 0xC9B7_3800;
pub const IMPACT_SHOWN_AS: u32 = 8;

/// The first factor of a fall's harm: its height less this, in meters.
const SPARED: f32 = 2.5;
/// The second factor: 1 more for each square meter of height above [`STEEPENS`], times this.
const STEEPENING: f32 = 0.001_23;
const STEEPENS: f32 = 12.5;

/// A player's falling: the last report taken, and the descent under way.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Falling {
    last: Option<Report>,
    descent: Option<Descent>,
}

/// A report taken: where, and when the client said.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Report {
    at: StoredPosition,
    time: f64,
}

/// A descent: its top, its lowest place yet, and its fastest falling step (0: it is no fall yet).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Descent {
    top: f32,
    low: f32,
    fastest: f32,
}

impl Falling {
    /// Is a fall under way: a descent one of whose steps went down as a fall does?
    #[must_use]
    pub fn falling(&self) -> bool {
        self.descent.is_some_and(|d| d.fastest > 0.0)
    }
}

/// The health a fall of `height` meters takes, mounted or not, before the fall modifier.
#[must_use]
pub fn harm(height: f32, mounted: bool) -> i32 {
    let h = height.min(TOP_SPEED_HEIGHT);
    let first = (h - SPARED).max(0.0);
    let second = if mounted {
        1.0
    } else {
        1.0 + STEEPENING * (h - STEEPENS).max(0.0).powi(2)
    };
    dereth_primitives::num::floor_to_i32(first * second)
}

/// A player's report has been taken (its place is the player's now; `before` its place until
/// then, `time` the time its client reported with it): a fall that lands harms it.
pub fn moved<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    before: &StoredPosition,
    time: f64,
    clock: Clock,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let at = p.position;
    let away = p.trip.is_some() || p.dying.is_some() || crate::combat::is_dead(p);
    let f = &mut p.fall;
    let last = f.last.replace(Report { at, time });
    let Some(last) = last.filter(|l| {
        !away
            && l.at.cell == before.cell
            && l.at.origin == before.origin
            && time > l.time
            && time - l.time <= GAP
    }) else {
        f.descent = None;
        return;
    };
    let z = at.origin[2];
    let down = last.at.origin[2] - z;
    if down <= DOWN {
        // It goes down no further: a fall lands at its lowest place.
        if let Some(d) = f.descent.take().filter(|d| d.fastest > 0.0) {
            land(s, net, world, d.top - d.low.min(z), clock);
        }
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let dt = (time - last.time) as f32;
    let speed = down / dt;
    let all = presence::distance(&last.at, &at).unwrap_or(down);
    let across = (all * all - down * down).max(0.0).sqrt() / dt;
    let d = f.descent.get_or_insert(Descent {
        top: last.at.origin[2],
        low: z,
        fastest: 0.0,
    });
    d.low = z;
    if d.fastest > 0.0 && speed < d.fastest / 2.0 {
        // It met the ground and slides on: the fall lands here, and what follows is a new descent.
        let height = d.top - z;
        *d = Descent {
            top: z,
            low: z,
            fastest: 0.0,
        };
        land(s, net, world, height, clock);
        return;
    }
    if speed >= FALLING && speed >= STEEP * across {
        if d.fastest <= 0.0 {
            // It falls from where a body dropping from rest would have been: no higher than the
            // descent's top, and no higher than its speed at the step's end says.
            let since = speed / GRAVITY + dt / 2.0;
            d.top = d.top.min(z + GRAVITY / 2.0 * since * since);
        }
        d.fastest = d.fastest.max(speed);
    }
}

/// A fall of `height` meters lands: water spares the player; a fall that would harm it, Safe Fall
/// spares, and ends; else it takes the fall's harm.
fn land<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    height: f32,
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if in_water(s, &p.position) {
        return;
    }
    let modifier = quals::float(&p.qualities, FALL_MODIFIER).unwrap_or(1.0);
    #[allow(clippy::cast_precision_loss)]
    let points = dereth_primitives::num::floor_to_i32(
        harm(height, crate::mounts::mounted(p)) as f32 * modifier.max(0.0),
    );
    if points <= 0 {
        return;
    }
    let me = p.character;
    let safe = p
        .play
        .effects
        .iter()
        .find(|e| {
            s.effect_table
                .get(e.state)
                .is_some_and(|d| d.enum_value == SAFE_FALL)
        })
        .map(|e| e.eid);
    if let Some(eid) = safe {
        crate::effects::remove(s, net, world, eid);
        return;
    }
    let before = health_of(s, world);
    let (share, was) = crate::combat::lose_health(s, net, world, points);
    let dead = s.players.get(&world).is_some_and(crate::combat::is_dead);
    let lost = before - health_of(s, world);
    crate::combat::wounded(s, net, me, share, was);
    if !dead && lost > 0 {
        let said = crate::progress::text_with(
            IMPACT,
            vec![crate::progress::word(
                IMPACT_POINTS,
                2,
                lost.cast_unsigned(),
            )],
        );
        crate::progress::private(net, world, &crate::chat::show_text(IMPACT_SHOWN_AS, said));
    }
    if dead {
        crate::death::player_dies(s, net, world, None, clock);
    }
}

/// A player's health now (its greatest where none is set, as a hurt takes it).
fn health_of<S: Store>(s: &Server<S>, world: ConnId) -> i32 {
    use crate::player::quality;
    s.players.get(&world).map_or(0, |p| {
        quals::int(&p.qualities, quality::HEALTH)
            .unwrap_or_else(|| crate::effects::own_greatest(s, p, quality::HEALTH_MAX).max(1))
    })
}

/// Is a place outdoors below its block's water surface?
fn in_water<S: Store>(s: &Server<S>, at: &StoredPosition) -> bool {
    !presence::indoors(at.cell)
        && s.ground
            .as_ref()
            .and_then(|g| g.water(at.cell))
            .is_some_and(|surface| at.origin[2] < surface)
}
