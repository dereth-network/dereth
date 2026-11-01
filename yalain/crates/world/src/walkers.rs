//! Placed people who walk a route: the Sentries pacing their lines.
//!
//! A person the world database places may name a route, the waypoint it sets out toward and which
//! way it goes on. It walks the route as a patrolling creature walks its own: on to the waypoint it
//! is going to, at the route's gait and its own scale (a walk of 1.62 m/s at scale 1, 2017's Sentries
//! 1.62 to 1.63 m/s), told standing there, resting the waypoint's rest, then on to the next, back
//! and forth or round a loop. Its place is told to everyone who sees it as it goes, a person new to
//! a client made where it stands now. Where the walls are known it moves only while a player is in
//! its landblock or one beside it, as creatures do. A person walks the straight lines between its
//! waypoints (the lines 2017's people walked); it does not fight and never leaves its route.

use std::collections::BTreeMap;

use auberean_protocol::physics::ObjectPosition;
use auberean_protocol::types::ObjectStamp;
use yalain_net::ServerNet;
use yalain_store::{Store, StoredPosition};

use crate::ai::{self, Patrol};
use crate::{presence, Clock, Server};

/// The instance stamp the world's own objects are made with.
const INSTANCE: u16 = 1;

/// One person on its route: the content object (by index), its way along the route, and the
/// stamps and motion its places are told with.
#[derive(Debug, Clone, PartialEq)]
pub struct Walker {
    pub index: usize,
    pub patrol: Patrol,
    pub position_stamp: u16,
    pub heading_motion: u32,
}

/// The people walking their routes, by their ids.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Walkers {
    pub by_id: BTreeMap<u64, Walker>,
}

/// Keep the walkers to the world's people with routes: a person newly placed with one sets out
/// from where it stands, one no longer placed (the world database reloaded) is let go.
fn keep<S: Store>(s: &mut Server<S>) {
    let objects = &s.content.objects;
    s.walkers.by_id.retain(|id, w| {
        objects.get(w.index).is_some_and(|o| o.walks.is_some())
            && presence::content_id(objects, w.index) == *id
    });
    for (i, o) in objects.iter().enumerate() {
        let Some(m) = &o.walks else { continue };
        let Some(route) = &m.route else { continue };
        let id = presence::content_id(objects, i);
        s.walkers.by_id.entry(id).or_insert_with(|| Walker {
            index: i,
            patrol: Patrol::new(route.clone(), m.start, m.backward),
            position_stamp: 1,
            heading_motion: crate::world::IDLE_MOTION,
        });
    }
}

/// Every person on a route takes its step.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    keep(s);
    let ids: Vec<u64> = s.walkers.by_id.keys().copied().collect();
    for id in ids {
        walk(s, net, id, clock);
    }
}

/// Where a person stands now.
fn at<S: Store>(s: &Server<S>, index: usize) -> Option<StoredPosition> {
    s.content.objects.get(index).map(|o| StoredPosition {
        cell: o.cell,
        origin: o.origin,
        rotation: o.rotation,
    })
}

fn walk<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let now = clock.local.0;
    let Some(w) = s.walkers.by_id.get(&id) else {
        return;
    };
    if now < w.patrol.rests_until {
        return;
    }
    let (index, walking) = (w.index, w.patrol.walking);
    let (Some(goal), Some(here)) = (w.patrol.goal(), at(s, index)) else {
        return;
    };
    let (dx, dy, dz) = ai::offset(&here, &goal);
    let flat = (dx * dx + dy * dy).sqrt();
    if flat <= ai::ARRIVED {
        // There: told standing, it rests the waypoint's rest and turns to the next.
        if let Some(w) = s.walkers.by_id.get_mut(&id) {
            let p = &mut w.patrol;
            let rest = p.route.waypoints.get(p.to).map_or(0.0, |x| x.rest);
            (p.to, p.backward) = p.route.next(p.to, p.backward);
            p.rests_until = now + rest;
            p.walking = false;
        }
        if walking {
            #[allow(clippy::cast_possible_truncation)]
            let h = (s.walkers.by_id[&id].heading_motion >> 24) as u8;
            place(s, net, id, here, h, crate::world::IDLE_MOTION, clock);
        }
        return;
    }
    if s.walls.is_some() && !ai::player_near(s, &here) {
        return;
    }
    let route = &s.walkers.by_id[&id].patrol.route;
    let (speed, motion) = match route.gait {
        yalain_content::movement::Gait::Walk => (ai::PATROL_WALK_SPEED, ai::PATROL_WALKING),
        yalain_content::movement::Gait::Run => (ai::RUN_SPEED, ai::RUNNING),
    };
    let scale = s.content.objects[index].description.scale.unwrap_or(1.0);
    let speed = route.speed.unwrap_or(speed) * scale;
    #[allow(clippy::cast_possible_truncation)]
    let go = (speed * crate::spawn::STEP_SECONDS as f32).min(flat);
    let (x, y) = (
        here.origin[0] + dx / flat * go,
        here.origin[1] + dy / flat * go,
    );
    let (cell, x, y, z) = if presence::indoors(here.cell) {
        let cell = if flat - go < 3.0 {
            goal.cell
        } else {
            here.cell
        };
        (cell, x, y, here.origin[2] + dz * (go / flat))
    } else {
        let (cell, x, y) = crate::spawn::outdoor(here.cell >> 16, x, y);
        let z = s
            .ground
            .as_ref()
            .and_then(|g| g.height(cell, [x, y, here.origin[2]]))
            .unwrap_or(here.origin[2] + dz * (go / flat));
        (cell, x, y, z)
    };
    if let Some(w) = s.walkers.by_id.get_mut(&id) {
        w.patrol.walking = true;
    }
    let h = ai::heading_toward(dx, dy);
    let next = StoredPosition {
        cell,
        origin: [x, y, z],
        rotation: crate::world::rotation_of_heading(h),
    };
    place(s, net, id, next, h, motion, clock);
}

/// A person takes a new place: kept as its content object's, and told to everyone who sees it.
fn place<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    to: StoredPosition,
    heading: u8,
    motion: u32,
    clock: Clock,
) {
    let time = net.game_time(clock.local);
    let Some(w) = s.walkers.by_id.get_mut(&id) else {
        return;
    };
    w.heading_motion = u32::from(heading) << 24 | (motion & 0x00FF_FFFF);
    w.position_stamp = w.position_stamp.wrapping_add(1);
    let (index, heading_motion, position_stamp) = (w.index, w.heading_motion, w.position_stamp);
    let Some(o) = s.content.objects.get_mut(index) else {
        return;
    };
    let new_cell = o.cell != to.cell;
    o.cell = to.cell;
    o.origin = to.origin;
    o.rotation = to.rotation;
    let m = ObjectPosition {
        sender: ObjectStamp {
            id,
            instance_stamp: INSTANCE,
            other_stamp: 0,
        },
        time,
        cell: to.cell,
        origin: auberean_primitives::Vec3::new(to.origin[0], to.origin[1], to.origin[2]),
        heading_motion,
        flags: ai::ON_GROUND,
        position_stamp,
        ..ObjectPosition::default()
    };
    presence::content_moves(s, net, index, &m, new_cell);
}

/// The position stamp a walking person's place was last told with, for a client it is made for.
#[must_use]
pub fn position_stamp<S: Store>(s: &Server<S>, id: u64) -> Option<u16> {
    s.walkers.by_id.get(&id).map(|w| w.position_stamp)
}
