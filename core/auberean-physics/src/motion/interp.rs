//! Following the positions the server sends for a body: the interpolation manager keeps them as a
//! path of keyframes, each with the distance along the path to it, and each tick steers the body
//! along the path at its own speed, toward a point one tick's full-speed travel past the closest
//! point. It hurries when far behind, slows as the path asks, applies jumps and pushes where the
//! path holds them, and puts the body at the path's end at once ("blips") when it falls behind for
//! good or cannot apply an impulse.

// Comparisons are written as the client makes them, so a value that is not a number goes the way it
// goes there.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::collections::VecDeque;

use dereth_physics::V3;
use dereth_primitives::{Position, Vec3};

use crate::geom::{self, Heading, EPS};
use crate::motion::moveto::Steer;
use crate::motion::{MotionInterp, Owner, AIR};

/// Within this of an impulse's keyframe the impulse is applied.
const IMPULSE_REACH: f32 = 0.100_000_01;
/// How long an impulse may wait to apply before the body is put at the path's end.
const IMPULSE_PATIENCE: f32 = 3.0;
/// How long the body may make no headway along the path.
const STALL_LIMIT: f32 = 2.0;
/// The squared distance from the path's end beyond which the body has fallen too far behind.
const BEHIND_LIMIT_SQ: f32 = 225.0;

/// One position the server sent, as a keyframe (copied whole, as the client copies it).
// D2-07071
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    pub pos: Position,
    /// The server's time it was sent.
    pub time: f64,
    /// The motion it asked for (sideways, forward, vertical, each -1 to 1).
    pub motion: Vec3,
    pub impulse: bool,
    pub impulse_stamp: u32,
    pub jump: bool,
    pub contact: bool,
    pub velocity: Vec3,
    /// The distance along the path to it.
    pub dist: f32,
}

/// The path of keyframes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Trail {
    /// The last impulse applied.
    pub last_impulse_stamp: u32,
    /// The last impulse queued.
    pub next_impulse_stamp: u32,
    pub queue: VecDeque<Keyframe>,
}

impl Trail {
    fn pending(&self, k: &Keyframe) -> bool {
        k.impulse && self.last_impulse_stamp < k.impulse_stamp
    }

    /// The keyframes around a distance along the path: the first at or past it (or the first
    /// impulse not yet applied), and the one before (the same when it is the first). The first
    /// may be past the end.
    // D2-07077
    #[must_use]
    pub fn keyframes_for(&self, d: f32) -> Option<(usize, usize)> {
        if !(d >= 0.0) || self.queue.is_empty() {
            return None;
        }
        let mut b = 0;
        while b < self.queue.len() {
            let k = &self.queue[b];
            if d <= k.dist || self.pending(k) {
                break;
            }
            b += 1;
        }
        let a = if b == 0 { 0 } else { b - 1 };
        Some((a, b))
    }

    /// The point at a distance along the path: on the segment it falls in, by distance; past the
    /// end, the last keyframe moved on along its motion; held at an impulse not yet applied.
    // D2-07078
    #[must_use]
    pub fn point_from_distance(&self, d: f32) -> Option<Position> {
        let (a, b) = self.keyframes_for(d)?;
        let ka = &self.queue[a];
        let d = d - ka.dist;
        if self.pending(ka) {
            return Some(ka.pos);
        }
        let mut p = ka.pos;
        let Some(kb) = self.queue.get(b) else {
            geom::add_local_offset(
                &mut p,
                Vec3::new(d * ka.motion.x, d * ka.motion.y, d * ka.motion.z),
            );
            return Some(p);
        };
        let o = geom::offset(&ka.pos, &kb.pos);
        let len = (o.x * o.x + o.y * o.y + o.z * o.z).sqrt();
        let step = if len >= EPS {
            let k = 1.0 / len;
            Vec3::new(k * o.x * d, o.y * k * d, o.z * k * d)
        } else {
            Vec3::ZERO
        };
        let seg = kb.dist - ka.dist;
        let t = if seg <= 0.0 {
            0.0
        } else {
            let r = (step.x * step.x + step.y * step.y + step.z * step.z).sqrt() / seg;
            unit(r)
        };
        p.frame.origin = geom::lerp_origin(&ka.pos, &kb.pos, t);
        geom::slerp_rotation(&mut p.frame, &ka.pos.frame, &kb.pos.frame, t);
        Some(p)
    }

    /// Whether the keyframe before a distance along the path touched the ground.
    // D2-07079
    #[must_use]
    pub fn has_contact(&self, d: f32) -> bool {
        self.keyframes_for(d)
            .is_some_and(|(a, _)| self.queue[a].contact)
    }

    /// The impulse to apply at a distance along the path, if the next one is within reach: its
    /// place, its velocity and whether it is a jump.
    // D2-07080
    #[must_use]
    pub fn should_impulse_here(&self, d: f32) -> Option<(Position, Vec3, bool)> {
        let (a, b) = self.keyframes_for(d)?;
        let ka = &self.queue[a];
        let next = self.last_impulse_stamp.wrapping_add(1);
        let b_stamp = self.queue.get(b).map(|k| k.impulse_stamp);
        // The keyframe's own impulse, numbered by the one after it.
        if ka.impulse && b_stamp == Some(next) && (d - ka.dist).abs() < IMPULSE_REACH {
            return Some((ka.pos, ka.velocity, ka.jump));
        }
        if let Some(kb) = self.queue.get(b) {
            if kb.impulse && kb.impulse_stamp == next && (d - kb.dist).abs() < IMPULSE_REACH {
                return Some((kb.pos, kb.velocity, ka.jump));
            }
        }
        None
    }

    /// The motion asked for at a distance along the path, blended between its keyframes, and the
    /// heading of the keyframe before it.
    // D2-07081
    #[must_use]
    pub fn interpolated_values_for(&self, d: f32) -> Option<(Vec3, Heading)> {
        let (a, b) = self.keyframes_for(d)?;
        let ka = &self.queue[a];
        let v = match self.queue.get(b) {
            None => ka.motion,
            Some(kb) => {
                let d = d - ka.dist;
                let o = geom::offset(&ka.pos, &kb.pos);
                let len = (o.x * o.x + o.y * o.y + o.z * o.z).sqrt();
                let t = if len <= 0.0 {
                    0.0
                } else {
                    let r = d / len;
                    unit(r)
                };
                Vec3::new(
                    (kb.motion.x - ka.motion.x) * t + ka.motion.x,
                    (kb.motion.y - ka.motion.y) * t + ka.motion.y,
                    (kb.motion.z - ka.motion.z) * t + ka.motion.z,
                )
            }
        };
        Some((v, geom::frame_heading(&ka.pos.frame)))
    }

    /// The point on the path closest to the body, the distance along the path to it, and whether
    /// it lies on the last stretch (the last keyframe carried on along its motion since it was
    /// sent). The search stops at an impulse not yet applied.
    // D2-07082
    pub fn find_closest_point(
        &self,
        mi: &MotionInterp,
        owner: &dyn Owner,
        now: f64,
        pos: &Position,
    ) -> Option<(Position, f32, bool)> {
        let first = self.queue.front()?;
        let mut best = f32::MAX;
        let mut found: Option<(Position, f32, bool)> = None;
        for i in 0..self.queue.len() {
            let e = &self.queue[i];
            if self.pending(e) {
                return Some((e.pos, e.dist, false));
            }
            let last = i + 1 == self.queue.len();
            let b = if last {
                let v = mi.local_desired(owner, e.motion, false, true);
                #[allow(clippy::cast_possible_truncation)]
                let t = (now - e.time) as f32;
                let mut b = e.pos;
                geom::add_local_offset(&mut b, Vec3::new(v.x * t, v.y * t, v.z * t));
                b
            } else {
                self.queue[i + 1].pos
            };
            let ab = geom::offset(&e.pos, &b);
            let ap = geom::offset(&e.pos, pos);
            let len2 = ab.y * ab.y + ab.z * ab.z + ab.x * ab.x;
            let t = if len2 <= 0.0 {
                0.0
            } else {
                let r = (ap.y * ab.y + ap.z * ab.z + ap.x * ab.x) / len2;
                unit(r)
            };
            let p = Vec3::new(ab.x * t, ab.y * t, ab.z * t);
            let on = e.pos.frame.origin.add(p);
            // Measured against the body's own coordinates, whatever its block.
            let q = pos.frame.origin;
            let d2 = (q.y - on.y) * (q.y - on.y)
                + (q.z - on.z) * (q.z - on.z)
                + (q.x - on.x) * (q.x - on.x);
            if d2 <= best {
                best = d2;
                let seg = (p.y * p.y + p.z * p.z + p.x * p.x).sqrt();
                let back = geom::offset(&b, &e.pos);
                let span = (back.z * back.z + back.y * back.y + back.x * back.x).sqrt();
                let f = if span <= 0.0 {
                    0.0
                } else {
                    let r = seg / span;
                    unit(r)
                };
                let mut c = e.pos;
                c.frame.origin = geom::lerp_origin(&e.pos, &b, f);
                geom::slerp_rotation(&mut c.frame, &e.pos.frame, &b.frame, f);
                found = Some((c, seg + e.dist, last));
            }
            if !last && self.pending(&self.queue[i + 1]) {
                break;
            }
        }
        Some(found.unwrap_or((first.pos, 0.0, false)))
    }

    // D2-07089
    #[must_use]
    pub fn last(&self) -> Option<&Keyframe> {
        self.queue.back()
    }

    /// The distance along the path to its last keyframe.
    // D2-07090
    #[must_use]
    pub fn total_distance(&self) -> f32 {
        self.queue.back().map_or(0.0, |k| k.dist)
    }

    /// The path's length with the last keyframe carried on along its motion since it was sent.
    // D2-07091
    #[must_use]
    pub fn interpolated_distance(&self, mi: &MotionInterp, owner: &dyn Owner, now: f64) -> f32 {
        let Some(last) = self.queue.back() else {
            return 0.0;
        };
        #[allow(clippy::cast_possible_truncation)]
        let t = (now - last.time).abs() as f32;
        let v = mi.local_desired(owner, last.motion, false, true);
        (v.z * v.z + v.y * v.y + v.x * v.x).sqrt() * t + self.total_distance()
    }

    /// Drops the keyframes passed by a distance along the path (keeping the one before it, and
    /// any impulse not yet applied), then numbers the rest anew: each takes the length of the
    /// stretch before it, as the client does, not the running total. How many were dropped.
    // D2-07114
    pub fn throw_away(&mut self, d: f32) -> usize {
        if !(d >= 0.0) {
            return 0;
        }
        let passed = self.queue.iter().take_while(|k| !(d < k.dist)).count();
        if passed == 0 {
            return 0;
        }
        let mut left = passed - 1;
        while left != 0 {
            let Some(front) = self.queue.front() else {
                break;
            };
            if front.impulse && front.impulse_stamp > self.last_impulse_stamp {
                break;
            }
            self.queue.pop_front();
            left -= 1;
        }
        let mut acc = 0.0;
        for i in 0..self.queue.len() {
            self.queue[i].dist = acc;
            if i + 1 < self.queue.len() {
                acc = geom::distance(&self.queue[i].pos, &self.queue[i + 1].pos);
            }
        }
        (passed - 1) - left
    }

    /// Queues a keyframe: one older than the last is dropped, an impulse already applied too; one
    /// in the same place as the last is folded into it.
    // D2-07121
    pub fn add(&mut self, k: Keyframe) {
        if k.impulse && k.impulse_stamp <= self.last_impulse_stamp {
            return;
        }
        let mut dist = 0.0;
        if let Some(back) = self.queue.back_mut() {
            if k.time < back.time {
                return;
            }
            let step = geom::distance(&back.pos, &k.pos);
            dist = back.dist + step;
            if step < EPS && (!back.impulse || !k.impulse) {
                geom::set_heading(&mut back.pos.frame, geom::frame_heading(&k.pos.frame));
                back.time = k.time;
                back.motion = k.motion;
                back.contact = k.contact;
                if !k.impulse {
                    return;
                }
                back.impulse = true;
                back.impulse_stamp = k.impulse_stamp;
                back.velocity = k.velocity;
                back.jump = k.jump;
                return;
            }
        }
        self.queue.push_back(Keyframe { dist, ..k });
    }
}

/// A body's interpolation toward the positions the server sends.
// D2-07123
#[derive(Clone, Debug, PartialEq)]
pub struct Follower {
    pub active: bool,
    /// Set by an impulse until the body touches the ground again.
    pub waiting_for_contact: bool,
    pub path: Trail,
    /// The motion asked for last tick.
    pub last_do_motion: Vec3,
    /// How long the body has made no headway along the path.
    pub stalled: f32,
    /// How long an impulse has waited to apply.
    pub waiting: f32,
    pub last_server_blip: f64,
    pub server_blips: u32,
    pub debug_offset: Vec3,
    pub debug_normal: Vec3,
}

impl Default for Follower {
    // D2-07122, D2-07124, D2-07066
    fn default() -> Self {
        Self {
            active: false,
            waiting_for_contact: false,
            path: Trail::default(),
            last_do_motion: Vec3::ZERO,
            stalled: 0.0,
            waiting: 0.0,
            last_server_blip: -1.0,
            server_blips: 0,
            debug_offset: Vec3::ZERO,
            debug_normal: Vec3::ZERO,
        }
    }
}

/// Each of a motion's parts held to -1 to 1.
fn clamp_motion(m: Vec3) -> Vec3 {
    let c = |v: f32| {
        let v = if v >= 1.0 { 1.0 } else { v };
        if v <= -1.0 {
            -1.0
        } else {
            v
        }
    };
    Vec3::new(c(m.x), c(m.y), c(m.z))
}

impl Follower {
    // D2-07067
    pub fn stop_interpolating(&mut self) {
        self.active = false;
        self.last_do_motion = Vec3::ZERO;
    }

    /// The motion the newest keyframe asks for while interpolating.
    // D2-07094
    #[must_use]
    pub fn last_received_motion(&self) -> Vec3 {
        if self.active {
            self.path.last().map_or(Vec3::ZERO, |k| k.motion)
        } else {
            Vec3::ZERO
        }
    }

    fn start(&mut self) {
        if !self.active {
            self.path.queue.clear();
            self.path.last_impulse_stamp = 0;
            self.path.next_impulse_stamp = 0;
            self.waiting = 0.0;
            self.stalled = 0.0;
            self.active = true;
        }
    }

    fn keyframe(time: f64, p: Position, motion: Vec3, heading: Heading) -> Keyframe {
        let mut pos = p;
        geom::set_heading(&mut pos.frame, heading);
        Keyframe {
            pos,
            time,
            motion: clamp_motion(motion),
            impulse: false,
            impulse_stamp: 0,
            jump: false,
            contact: false,
            velocity: Vec3::ZERO,
            dist: 0.0,
        }
    }

    /// A position the server sent: the place, facing the heading, asking for the motion.
    // D2-07126
    pub fn add_event(
        &mut self,
        time: f64,
        p: Position,
        motion: Vec3,
        heading: Heading,
        contact: bool,
    ) {
        self.start();
        let k = Keyframe {
            contact,
            ..Self::keyframe(time, p, motion, heading)
        };
        self.path.add(k);
    }

    /// A position the server sent with an impulse (its velocity, and whether it is a jump),
    /// numbered after the last.
    // D2-07125
    pub fn add_impulse_event(
        &mut self,
        time: f64,
        p: Position,
        motion: Vec3,
        heading: Heading,
        velocity: Vec3,
        jump: bool,
    ) {
        self.start();
        self.path.next_impulse_stamp = self.path.next_impulse_stamp.wrapping_add(1);
        let k = Keyframe {
            impulse: true,
            impulse_stamp: self.path.next_impulse_stamp,
            jump,
            contact: true,
            velocity,
            ..Self::keyframe(time, p, motion, heading)
        };
        self.path.add(k);
    }

    /// Steers toward a point on the path: the motion toward it in the body's frame, at the
    /// keyframes' asked speed (no less than half and no more than full), or a fifth faster when
    /// far behind; at the end of a short, still path, stops there and drops the path. Whether the
    /// body has arrived.
    // D2-07115
    #[allow(clippy::too_many_arguments)]
    pub fn seek(
        &mut self,
        mi: &MotionInterp,
        owner: &dyn Owner,
        now: f64,
        pos: &Position,
        along: f32,
        dist_closest: f32,
        target: &Position,
        steer: &mut Steer,
    ) -> bool {
        let off = geom::offset(pos, target);
        steer.heading = geom::heading_to(pos, target);
        let local = geom::global_to_local(&pos.frame, off);
        let len = (local.x * local.x + local.z * local.z + local.y * local.y).sqrt();
        let mut motion = if len < EPS {
            Vec3::ZERO
        } else {
            let k = 1.0 / len;
            Vec3::new(local.x * k, local.y * k, local.z * k)
        };
        let Some(last) = self.path.last().copied() else {
            steer.motion = motion;
            return false;
        };
        let to_end = geom::offset(&last.pos, pos);
        let d2 = to_end.x * to_end.x + to_end.y * to_end.y + to_end.z * to_end.z;
        let m = last.motion;
        let mut arrived = false;
        if last.dist >= 4.0 || m.z * m.z + m.y * m.y + m.x * m.x >= 0.01 {
            if self.path.interpolated_distance(mi, owner, now) >= 3.0 || dist_closest >= 1.0 {
                motion = Vec3::new(motion.x * 1.2, motion.y * 1.2, motion.z * 1.2);
            } else if let Some((v, h)) = self.path.interpolated_values_for(along) {
                steer.heading = h;
                let f = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
                let f = clamp(f, 0.5, 1.0);
                motion = Vec3::new(f * motion.x, f * motion.y, f * motion.z);
            }
        } else {
            if d2 < 0.0025 {
                let total = self.path.total_distance();
                self.path.throw_away(total);
                arrived = true;
            }
            if d2 < 0.5 {
                steer.heading = geom::frame_heading(&last.pos.frame);
            }
            if arrived {
                steer.motion = Vec3::ZERO;
                return true;
            }
        }
        if off.x.abs() < 0.1
            && off.y.abs() < 0.1
            && off.z.abs() >= EPS
            && mi.motion_values(owner).max_vertical == 0.0
        {
            motion = Vec3::new(0.0, 1.0, 0.0);
        }
        steer.motion = motion;
        arrived
    }

    /// Whether the body is getting along the path: it has passed a keyframe within two seconds,
    /// and is within 15 m of the path's end.
    // D2-07116
    pub fn check_progress(
        &mut self,
        mi: &MotionInterp,
        owner: &dyn Owner,
        dt: f32,
        pos: &Position,
        lookahead: f32,
        motion: Vec3,
    ) -> bool {
        let _ = mi.local_desired(owner, motion, false, true);
        if self.path.throw_away(lookahead) == 0 {
            self.stalled += dt;
            if self.stalled >= STALL_LIMIT {
                return false;
            }
        } else {
            self.stalled = 0.0;
        }
        let Some(last) = self.path.last() else {
            return true;
        };
        let o = geom::offset(pos, &last.pos);
        o.x * o.x + o.y * o.y + o.z * o.z <= BEHIND_LIMIT_SQ
    }

    /// Puts the body at the path's end at once, moving as the last keyframe asks, and stops
    /// interpolating.
    // D2-07093
    pub fn blip(&mut self, mi: &MotionInterp, owner: &mut dyn Owner, steer: &mut Steer) {
        if !self.active {
            return;
        }
        self.active = false;
        let Some(last) = self.path.last().copied() else {
            return;
        };
        owner.blip_to(last.pos);
        steer.motion = last.motion;
        steer.heading = geom::frame_heading(&last.pos.frame);
        // The asked velocity, in the body's frame, given as the body's velocity.
        let v = mi.local_desired(&*owner, last.motion, false, true);
        owner.set_velocity(v);
    }

    /// At an impulse's keyframe: the body is put there and given the impulse's velocity (a jump
    /// capped at the body's speeds and made through its jump); until it lands it waits for
    /// contact. An impulse that will not apply for three seconds blips the body. Either way it
    /// stops asking for motion this tick and faces as the keyframe faces.
    // D2-07103
    #[allow(clippy::too_many_arguments)]
    pub fn do_impulse_logic(
        &mut self,
        mi: &MotionInterp,
        owner: &mut dyn Owner,
        dt: f32,
        pos: &Position,
        target: &Position,
        along: f32,
        steer: &mut Steer,
    ) {
        if !(geom::distance(pos, target) < IMPULSE_REACH) {
            return;
        }
        let Some((at, velocity, jump)) = self.path.should_impulse_here(along) else {
            return;
        };
        owner.blip_to(at);
        let took = if jump {
            let mut l = geom::global_to_local(&at.frame, velocity);
            mi.clamp_velocity(&*owner, &mut l);
            owner.set_velocity(geom::local_to_global(&at.frame, l));
            owner.jump_logic()
        } else {
            owner.apply_impulse(velocity)
        };
        if took {
            self.path.last_impulse_stamp = self.path.last_impulse_stamp.wrapping_add(1);
            self.waiting = 0.0;
            self.waiting_for_contact = true;
        } else {
            self.waiting += dt;
            if self.waiting > IMPULSE_PATIENCE {
                self.blip(mi, owner, steer);
                self.waiting = 0.0;
                self.path.last_impulse_stamp = self.path.last_impulse_stamp.wrapping_add(1);
            }
        }
        steer.motion = Vec3::ZERO;
        steer.heading = geom::frame_heading(&at.frame);
    }

    /// Keeps the body on the path: across the ground it may go no farther than to the target point
    /// and only toward it; standing on the path (not on its last stretch, not waiting to land) it
    /// follows the transition exactly, touching the ground as the path does. The body's
    /// constraint holds only while the path is still.
    // D2-07092
    #[allow(clippy::too_many_arguments)]
    pub fn do_constraint(
        &mut self,
        mi: &mut MotionInterp,
        owner: &mut dyn Owner,
        pos: &Position,
        target: &Position,
        at_end: bool,
        dist_closest: f32,
        contact: bool,
    ) {
        if let Some(c) = mi.constraint.as_mut() {
            c.enabled = self.path.total_distance() <= EPS;
        }
        let o = geom::offset(pos, target);
        owner.set_distance_cap((o.x * o.x + o.y * o.y).sqrt());
        let len = (o.x * o.x + o.y * o.y + 0.0).sqrt();
        let rail = if len < EPS {
            Vec3::ZERO
        } else {
            let k = 1.0 / len;
            Vec3::new(o.x * k, o.y * k, k * 0.0)
        };
        owner.set_velocity_rail(rail);
        if dist_closest < EPS && !at_end && !self.waiting_for_contact {
            owner.set_synchronize_transition(true);
            owner.set_contact(contact);
        } else {
            owner.set_synchronize_transition(false);
        }
    }

    /// One tick of interpolation: finds the closest point on the path, looks one tick's full-speed
    /// travel past it, steers there, applies any impulse due, blips the body when it is not
    /// getting along, and keeps it on the path. Done once arrived and still.
    // D2-07117, D2-07127
    pub fn perform_interpolation(
        &mut self,
        mi: &mut MotionInterp,
        owner: &mut dyn Owner,
        now: f64,
        dt: f32,
        steer: &mut Steer,
    ) {
        steer.instant = false;
        let pos = owner.position();
        let Some((closest, along, at_end)) = self.path.find_closest_point(mi, &*owner, now, &pos)
        else {
            return;
        };
        let full = mi.local_desired(&*owner, Vec3::new(0.0, 1.0, 0.0), false, true);
        let tick = Vec3::new(full.x * dt, full.y * dt, full.z * dt);
        let lookahead = (tick.z * tick.z + tick.y * tick.y + tick.x * tick.x).sqrt() + along;
        let target = self.path.point_from_distance(lookahead).unwrap_or(pos);
        let dist_closest = geom::distance(&pos, &closest);
        let arrived = self.seek(mi, &*owner, now, &pos, along, dist_closest, &target, steer);
        if owner.has_contact() {
            self.waiting_for_contact = false;
        }
        self.do_impulse_logic(mi, owner, dt, &pos, &target, along, steer);
        if self.waiting_for_contact && owner.desired_element() == AIR {
            steer.motion = Vec3::ZERO;
        } else if !arrived && !self.check_progress(mi, &*owner, dt, &pos, lookahead, steer.motion) {
            self.blip(mi, owner, steer);
        }
        self.last_do_motion = steer.motion;
        let contact = self.path.has_contact(along);
        self.do_constraint(mi, owner, &pos, &target, at_end, dist_closest, contact);
        let v = owner.velocity();
        if arrived && v.z * v.z + v.y * v.y + v.x * v.x < EPS {
            self.active = false;
        }
    }
}

/// A ratio held to 0 to 1 (one that is not a number stays so).
#[allow(clippy::manual_clamp)]
fn unit(r: f32) -> f32 {
    if r >= 1.0 {
        1.0
    } else if r <= 0.0 {
        0.0
    } else {
        r
    }
}

/// `v` held between `lo` and `hi`.
// D2-07070
#[must_use]
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    let v = if hi <= v { hi } else { v };
    if lo < v {
        v
    } else {
        lo
    }
}
