//! Following: a player asks to follow an object (the client sends its selection) and is told so,
//! with one standing movement order toward it; the client keeps up on its own, and nothing is
//! sent again as the object moves. Asked again while following, the old follow ends first. An
//! attack or a use ends it: the order is stopped and the player told. The object followed is told
//! nothing.

use auberean_primitives::Vec3;
use auberean_protocol::types::{MoveTarget, MovementParams, Placement};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::Value;
use crate::{progress, Server};

/// The client's request to follow an object.
pub const FOLLOW: u32 = 0x8212_0010;

/// "Started following." and "Stopped following.", shown as plain text.
pub const STARTED: (u32, u32) = (0x5754_9382, 0x2500_000B);
pub const STOPPED: (u32, u32) = (0x5712_F382, 0x2500_000B);
pub const SHOWN_AS: u32 = 8;

/// How many follows a player may ask at once, and how many a second after that. (2017: 1,701 asked,
/// 7 in a second at most, none past these; our figures.)
pub const ASKS_AT_ONCE: f64 = 10.0;
/// See [`ASKS_AT_ONCE`].
pub const ASKS_PER_SECOND: f64 = 5.0;

/// The standing movement order's flags, and how near it keeps the player, in meters.
pub const ORDER_FLAGS: u32 = 0x6B;
pub const DISTANCE: f32 = 1.0;

/// A player asks to follow an object.
pub fn start<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(target) = v.iter().find_map(|x| match x {
        Value::Long(l) => Some(*l),
        _ => None,
    }) else {
        s.unhandled.note("a follow that does not read");
        return;
    };
    // Each order goes to everyone who sees the player: asked past the pace, nothing changes.
    let now = s.saves.now;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let asks = p
        .follows
        .get_or_insert_with(|| crate::call_limit::Bucket::with(ASKS_AT_ONCE, ASKS_PER_SECOND, now));
    if !asks.allow(now) {
        s.unhandled.note("a follow past the pace");
        return;
    }
    stop(s, net, world);
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let m = crate::places::move_order(
        p,
        MovementParams {
            flags: ORDER_FLAGS,
            desired_velocity: 1.0,
            fail_distance: f32::MAX,
            desired_distance: DISTANCE,
            max_success_distance: DISTANCE,
            min_success_distance: 0.0,
            target: MoveTarget::Object(target),
            offset: Vec3::ZERO,
            offset_id: 0,
            waypoints: Vec::new(),
            context,
        },
    );
    p.following = Some(target);
    let id = p.character;
    let said = crate::chat::show_text(SHOWN_AS, progress::text(STARTED));
    progress::private(net, world, &said);
    crate::places::to_all_knowing(s, net, id, &m);
}

/// A player following something stops: the order is stopped and the player told.
pub fn stop<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.following.take().is_none() {
        return;
    }
    let m = crate::places::move_order(
        p,
        MovementParams {
            flags: 0x40,
            desired_velocity: 0.0,
            fail_distance: 0.0,
            desired_distance: 0.0,
            max_success_distance: 0.0,
            min_success_distance: 0.0,
            target: MoveTarget::Place(Placement::default()),
            offset: Vec3::ZERO,
            offset_id: 0,
            waypoints: Vec::new(),
            context: 0,
        },
    );
    let id = p.character;
    crate::places::to_all_knowing(s, net, id, &m);
    let said = crate::chat::show_text(SHOWN_AS, progress::text(STOPPED));
    progress::private(net, world, &said);
}
