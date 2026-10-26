//! A body driven the way the client's travel drives the player's character: a turn key turns it at
//! the character's turn rate, the forward key runs it at the character's run speed, and the world
//! steps once a frame at the frame rate asked for. Every key the steering asks for takes effect for
//! the whole of the next frame, as a key the client reads at the end of one frame moves the body
//! in the next.

use std::sync::Arc;

use dereth_navigation::{FollowConfig, Follower, NavPath, Steer};
use dereth_physics::{MotionSource, PhysHandle, PhysicsWorld, SetupGeometry};
use dereth_primitives::num::math::{atan2f, cosf, sinf};
use dereth_primitives::{Frame, LocalTime, ObjectId, Position, Quat, Vec3};

/// The run speed of a character of modest run skill, metres a second.
pub const RUN_SPEED: f32 = 8.0;
/// The turn rate while running, degrees a second: a quarter turn a second, half as fast again
/// with the run key.
pub const TURN_RATE: f32 = 135.0;

/// The body's own movement for one frame: a steady velocity, in the world's frame.
#[derive(Debug)]
struct Push(Vec3);

impl MotionSource for Push {
    fn advance(&mut self, quantum: f64) -> Frame {
        #[allow(clippy::cast_possible_truncation)]
        let q = quantum as f32;
        Frame::new(
            Vec3::new(self.0.x * q, self.0.y * q, self.0.z * q),
            Quat::IDENTITY,
        )
    }
    fn tick_movement(&mut self, _now: LocalTime) {}
    fn hit_ground(&mut self) {}
    fn leave_ground(&mut self) {}
    fn process_hooks(&mut self) {}
    fn has_collision_geometry(&self) -> bool {
        true
    }
}

/// A body moved by keys.
pub struct KeyBody<'w> {
    world: &'w mut PhysicsWorld,
    handle: PhysHandle,
    /// The game clock.
    pub now: f64,
    /// Degrees clockwise from north.
    pub heading: f32,
}

impl<'w> KeyBody<'w> {
    /// A body of `setup` standing at `at`, facing `heading`; `None` when it cannot stand there.
    pub fn new(
        world: &'w mut PhysicsWorld,
        setup: Arc<SetupGeometry>,
        at: &Position,
        heading: f32,
    ) -> Option<Self> {
        let handle = world.create(ObjectId(0x5000_0002), setup, true);
        world.set_player(handle);
        let mut pos = *at;
        pos.frame.rotation = Quat::IDENTITY;
        if !world.set_position(handle, &pos) {
            world.destroy(handle);
            return None;
        }
        let mut b = Self {
            world,
            handle,
            now: 0.0,
            heading,
        };
        if let Some(o) = b.world.get_mut(handle) {
            o.velocity_vector = Vec3::ZERO;
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
            o.update_time = 0.0;
        }
        for _ in 0..8 {
            b.frame(1.0 / 30.0, false, None);
        }
        b.now = 0.0;
        Some(b)
    }

    /// Where the body stands.
    pub fn position(&self) -> Option<Position> {
        self.world.get(self.handle).map(|o| o.position)
    }

    /// One frame of `dt` seconds with the keys held: the turn first, then the run along the new
    /// heading.
    pub fn frame(&mut self, dt: f64, forward: bool, turn: Option<bool>) {
        #[allow(clippy::cast_possible_truncation)]
        let dtf = dt as f32;
        if let Some(right) = turn {
            let d = TURN_RATE * dtf;
            self.heading = (self.heading + if right { d } else { -d }).rem_euclid(360.0);
        }
        let v = if forward {
            let h = self.heading.to_radians();
            Vec3::new(sinf(h) * RUN_SPEED, cosf(h) * RUN_SPEED, 0.0)
        } else {
            Vec3::ZERO
        };
        if let Some(o) = self.world.get_mut(self.handle) {
            o.set_motion(Box::new(Push(v)));
            o.transient_state.set_active_bit(true);
        }
        self.now += dt;
        self.world.use_time(LocalTime(self.now), false);
    }
}

impl Drop for KeyBody<'_> {
    fn drop(&mut self) {
        self.world.destroy(self.handle);
    }
}

/// What driving a route came to.
#[derive(Debug, Clone, Default)]
pub struct Drive {
    pub arrived: bool,
    /// Seconds of game time.
    pub seconds: f64,
    /// The route's length over the run speed: the time a body that never stopped would take.
    pub ideal: f64,
    /// Times the forward key was let go before the end.
    pub stops: u32,
    /// Times the body stopped getting closer and the route was planned again.
    pub stuck: u32,
    /// Metres walked.
    pub walked: f32,
    /// Where it ended.
    pub end: Option<Position>,
}

/// Drive `path` at `fps` frames a second, planning again with `replan` from where the body is
/// (and the waypoint it was making for) when it gets stuck, at most eight times.
pub fn drive(
    world: &mut PhysicsWorld,
    setup: &Arc<SetupGeometry>,
    path: NavPath,
    fps: f64,
    replan: &mut dyn FnMut(&Position, &Position) -> Option<NavPath>,
) -> Drive {
    let mut out = Drive {
        ideal: f64::from(path.length() / RUN_SPEED),
        ..Drive::default()
    };
    let start = path.waypoints[0].position;
    let first = path.waypoints.get(1).map_or(start, |w| w.position);
    let g = |p: &Position| dereth_navigation::global(p);
    let (a, b) = (g(&start), g(&first));
    let heading = atan2f(b.x - a.x, b.y - a.y).to_degrees().rem_euclid(360.0);
    let Some(mut body) = KeyBody::new(world, Arc::clone(setup), &start, heading) else {
        return out;
    };
    let limit = out.ideal * 4.0 + 30.0;
    let dt = 1.0 / fps;
    let mut f = Follower::new(path, FollowConfig::default());
    let mut forward = false;
    let mut last = start;
    while body.now < limit {
        let Some(p) = body.position() else { break };
        out.walked += dereth_navigation::global_distance(&p, &last);
        last = p;
        match f.update(&p, body.heading, body.now) {
            Steer::Arrived => {
                out.arrived = true;
                break;
            }
            Steer::OpenDoor(_) => {
                f.door_opened();
                body.frame(dt, false, None);
            }
            Steer::Go {
                forward: go, turn, ..
            } => {
                if forward && !go {
                    out.stops += 1;
                }
                forward = go;
                body.frame(dt, go, turn);
            }
            Steer::Stuck { waypoint } => {
                let toward = f.path().waypoints[waypoint].position;
                out.stuck += 1;
                if out.stuck > 8 {
                    break;
                }
                match replan(&p, &toward) {
                    Some(path) => f = Follower::new(path, FollowConfig::default()),
                    None => break,
                }
                body.frame(dt, false, None);
            }
        }
    }
    out.seconds = body.now;
    out.end = body.position();
    out
}

/// Jump a body off a ledge: placed at `stand`, leaping toward `land` (a body cannot walk off a
/// ledge: it slides along the edge), and left to come down. Where it came to rest from `land`:
/// across and up or down; `None` when it could not stand at `stand`.
pub fn jump_off(
    world: &mut PhysicsWorld,
    setup: &Arc<SetupGeometry>,
    stand: &Position,
    land: &Position,
) -> Option<(f32, f32)> {
    let (a, b) = (
        dereth_navigation::global(stand),
        dereth_navigation::global(land),
    );
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    let mut body = KeyBody::new(world, Arc::clone(setup), stand, 0.0)?;
    if let Some(o) = body.world.get_mut(body.handle) {
        o.velocity_vector = Vec3::new(dx / l * 2.5, dy / l * 2.5, 2.0);
        o.transient_state.set_active_bit(true);
    }
    for _ in 0..240 {
        body.frame(1.0 / 30.0, false, None);
    }
    let at = dereth_navigation::global(&body.position()?);
    Some((
        ((at.x - b.x).powi(2) + (at.y - b.y).powi(2)).sqrt(),
        at.z - b.z,
    ))
}
