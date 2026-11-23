//! Following: a player asks to follow an object (the client sends its selection) and is told so,
//! with one standing movement order toward it; the client keeps up on its own, and nothing is
//! sent again as the object moves. Asked again while following, the old follow ends first.
//!
//! A follow ends, the order stopped for all who see the player and the player told, when the player
//! moves by its own hand or uses a skill: a skill's use that turns or walks the player to its
//! target ends it as it turns or walks, a refused one at its refusal, and one that gives the player
//! no movement order when it is over; a use the player's move cuts short ends it with its own end,
//! the player told last. A use asked while following ends it as the server takes it up, before the
//! use's walk or answer, except a use of something the player holds that walks nowhere, and a use
//! refused as busy, which leave it on; a held thing's use that walks ends it as the walk begins. A
//! run of attacks ends it as it is given its mode and its turn or its walk, or after its lowering
//! when it is refused at once; refused with its weapon still raised (its target too far, the player
//! moving), the run leaves it on until a later swing goes. The player's trip ends it: as it goes
//! where it is told "Teleporting ...", as it comes out where it is not. The player's own death ends
//! it too, the player told just after its death message.
//!
//! The object followed leaving the world ends every follow of it: a player logging off or losing
//! its connection, a body or a thing going. Its trip, its going out of the follower's view and its
//! death end none: the follow goes on until it leaves or the player acts (2017: 0 of 4 followed
//! players' deaths ended a follow; a creature is held to the same, until its body leaves, this
//! server's own rule, as no recording shows one). The object followed is told nothing, even when it
//! dies.

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

/// A player following something stops: the order is stopped and the player told. Returns whether
/// it was following.
pub fn stop<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) -> bool {
    let followed = order_stopped(s, net, world);
    if followed {
        told_stopped(net, world);
    }
    followed
}

/// A player following something follows no more: its order is stopped, and it is not told yet (a
/// use's end sends more before it does: [`told_stopped`]). Returns whether it was following.
pub(crate) fn order_stopped<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    if p.following.take().is_none() {
        return false;
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
    true
}

/// A player is told it stopped following.
pub(crate) fn told_stopped(net: &mut ServerNet, world: ConnId) {
    let said = crate::chat::show_text(SHOWN_AS, progress::text(STOPPED));
    progress::private(net, world, &said);
}

/// A following player reported moving by its own hand once it had taken up the order (its report
/// carries the order's stamp): the follow ends. Reports under the order change nothing. (2017: 742
/// follows ended so, a median 0.09 s after the first such report.)
pub fn moved<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    flags: u32,
    move_to_stamp: u16,
) {
    let by_hand = s.players.get(&world).is_some_and(|p| {
        p.following.is_some()
            && flags & crate::places::MOVED_BY_HAND != 0
            && move_to_stamp == p.move_to_stamp
    });
    if by_hand {
        stop(s, net, world);
    }
}

/// The object `id` left the world (a player logging off or losing its connection, a body or a thing
/// going): every player following it stops following, whether it still held it in view or not.
/// (2017: a follow ended as the followed player went to another world server and went on through
/// its trip on the same one. A log-off is that rule applied: none was recorded.)
pub fn target_left<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let followers: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(_, p)| p.following == Some(id))
        .map(|(w, _)| *w)
        .collect();
    for w in followers {
        stop(s, net, w);
    }
}

/// The object `id` was hidden from the players in `told` (a cloaked admin): those of them following
/// it stop following. Our own rule: no recording shows one.
pub(crate) fn target_hidden<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    told: &[ConnId],
) {
    for w in told {
        if s.players.get(w).is_some_and(|p| p.following == Some(id)) {
            stop(s, net, *w);
        }
    }
}

/// A player following something dies: it follows no more and is told so, just after its death
/// message (2017: 8 of 8). No movement order is sent with it (2017: the one stop of the order comes
/// later in the death's tick, for all who see it, 8 of 8).
pub fn died<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.following.take().is_none() {
        return;
    }
    told_stopped(net, world);
}
