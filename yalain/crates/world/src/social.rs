//! What players ask of one another: walked to the other first, then the asking carried out.
//!
//! **Swearing to a patron, recruiting a fellow and asking for a trade** each walk the asking player
//! to the other (a movement order toward the other's object: a meter between the bodies, given
//! up past a distance), and are carried out when the asker reports itself there, with the order
//! stopped; an asker already in reach is not walked.
//!
//! Also the pieces the allegiance, the fellowship and the trade share: a player's world
//! connection by its character, its name as the world writes it, chat-window texts.

use auberean_primitives::Vec3;
use auberean_protocol::types::{MoveTarget, MovementParams, Placement};
use auberean_protocol::ServerMessage;
use auberean_script::native_object::{TextInfo, Variable};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{move_order, to_all_knowing};
use crate::{progress, reach, Clock, Server};

/// What a player is walked to another player for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approach {
    /// To swear allegiance to it.
    Swear,
    /// To ask it into the fellowship.
    Recruit,
    /// To ask it for a trade.
    Trade,
}

/// The gap between the bodies the walk asks for, in meters.
pub const REACH: f32 = 1.0;

/// How far the walk may grow before it is given up: an oath's and a recruit's, a trade's.
pub const GIVE_UP: f32 = 30.0;
pub const GIVE_UP_TRADE: f32 = 6.0;

/// The client ends a walk by its own measure; a report this much past the reach still counts.
pub const SLACK: f32 = 1.0;

/// How many questions a player may put to others at once, and how many a second after that: an oath
/// offered, a place in a fellowship, a trade. (2017: the quickest two of 62 recruitments came 1.87 s
/// apart, of 101 trades asked 4.01 s, of 6 oaths none in a session; our figures.)
pub const ASKS_AT_ONCE: f64 = 5.0;
/// See [`ASKS_AT_ONCE`].
pub const ASKS_PER_SECOND: f64 = 1.0;

/// Chat-window kinds of the social texts: a refusal or a rank's change, a notice.
pub const REFUSED: u32 = 1;
pub const NOTICE: u32 = 8;

/// The world connection of a character in the world.
#[must_use]
pub fn world_of<S: Store>(s: &Server<S>, character: u64) -> Option<ConnId> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(w, _)| *w)
}

/// A player's name as the world writes it (with its grammar mark).
#[must_use]
pub fn name_of<S: Store>(s: &Server<S>, world: ConnId) -> String {
    s.players
        .get(&world)
        .map_or_else(String::new, |p| crate::player::world_name(&p.name, p.sex))
}

/// A name as a string reference holding it.
#[must_use]
pub fn name_text(name: &str) -> TextInfo {
    crate::profile::literal(name)
}

/// A variable holding a name.
#[must_use]
pub fn name_var(id: u32, name: &str) -> Variable {
    progress::nested(id, name_text(name))
}

/// Show a player a text of a kind.
pub fn show(net: &mut ServerNet, world: ConnId, kind: u32, t: TextInfo) {
    progress::private(net, world, &crate::chat::show_text(kind, t));
}

/// The gap between two players' bodies (none when they are far apart).
#[must_use]
pub fn gap<S: Store>(s: &Server<S>, a: ConnId, b: ConnId) -> Option<f32> {
    let (pa, pb) = (s.players.get(&a)?, s.players.get(&b)?);
    reach::gap(&pa.position, reach::PLAYER, &pb.position, reach::PLAYER)
}

fn order(target: u64, give_up: f32, flags: u32) -> MovementParams {
    MovementParams {
        flags,
        desired_velocity: if flags == WALK { 1.0 } else { 0.0 },
        fail_distance: if flags == WALK { give_up } else { 0.0 },
        desired_distance: if flags == WALK { REACH } else { 0.0 },
        max_success_distance: if flags == WALK { REACH } else { 0.0 },
        min_success_distance: 0.0,
        target: if flags == WALK {
            MoveTarget::Object(target)
        } else {
            MoveTarget::Place(Placement::default())
        },
        offset: Vec3::ZERO,
        offset_id: 0,
        waypoints: Vec::new(),
        context: 0,
    }
}

/// The movement order's flags: walk to an object, stop.
const WALK: u32 = 0x63;
const STOP: u32 = 0x40;

fn send_order<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    params: MovementParams,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let m: ServerMessage = move_order(p, params);
    let id = p.character;
    to_all_knowing(s, net, id, &m);
}

/// Walk a player to another for something, or carry it out at once when it is in reach.
pub fn approach<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    other: ConnId,
    what: Approach,
    clock: Clock,
) {
    let Some(target) = s.players.get(&other).map(|p| p.character) else {
        return;
    };
    // Each of these puts a question before the other player, and its walk goes to everyone who
    // sees the asker: past the pace neither happens.
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let asks = p.asks.get_or_insert_with(|| {
        crate::call_limit::Bucket::with(ASKS_AT_ONCE, ASKS_PER_SECOND, clock.local)
    });
    if !asks.allow(clock.local) {
        s.unhandled
            .note("a question to another player past the pace");
        return;
    }
    if gap(s, world, other).is_some_and(|g| g <= REACH + SLACK) {
        carry_out(s, net, world, other, what, clock);
        return;
    }
    let give_up = if what == Approach::Trade {
        GIVE_UP_TRADE
    } else {
        GIVE_UP
    };
    send_order(s, net, world, order(target, give_up, WALK));
    if let Some(p) = s.players.get_mut(&world) {
        p.approaching = Some((target, what));
    }
}

/// A player walking to another has reported its place: carry out what it walked for once it is
/// there (the walk stopped).
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some((target, what)) = s.players.get(&world).and_then(|p| p.approaching) else {
        return;
    };
    let Some(other) = crate::presence::player_for(s, world, target) else {
        if let Some(p) = s.players.get_mut(&world) {
            p.approaching = None;
        }
        return;
    };
    if gap(s, world, other).is_some_and(|g| g <= REACH + SLACK) {
        if let Some(p) = s.players.get_mut(&world) {
            p.approaching = None;
        }
        send_order(s, net, world, order(0, 0.0, STOP));
        carry_out(s, net, world, other, what, clock);
    }
}

fn carry_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    other: ConnId,
    what: Approach,
    clock: Clock,
) {
    match what {
        Approach::Swear => crate::allegiance::ask_patron(s, net, world, other),
        Approach::Recruit => crate::fellowship::ask_recruit(s, net, world, other, clock),
        Approach::Trade => crate::trade::register(s, net, world, other, clock),
    }
}
