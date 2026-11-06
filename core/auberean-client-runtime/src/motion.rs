//! The player's body as the client moves it, and when the client tells the server where it is.
//!
//! **The body.** It stands where the server last put it (its creation, a teleport, a forced
//! position) and moves as the player asks: forward, back and sideways at the movement speeds, facing
//! the heading the player turns it to. On its own this client moves it without the world's
//! collisions; a host with the world's physics moves it itself and tells the body where it is
//! ([`Body::follow`]).
//!
//! **The report.** Once a frame the client decides whether to send where the body is: always after
//! a jump; when the asked motion changed by 0.0002 or more on any axis; otherwise only after more
//! than 0.2 seconds since the last report, and then only if the body changed cell, moved 0.0002 or
//! more on any axis, or turned. The report carries the game clock, the cell and the body's place in
//! it, the heading (in the top byte, 255 to a full turn) and the asked motion (sideways, forward and
//! up, a byte each, 127 for none), the flags (2 a jump, with its velocity; 4 the player is moving or
//! turning this frame), and the body's instance, teleport, forced-position and move-to stamps.

use auberean_client_contract::Motion;
use auberean_physics::geom::Heading;
use auberean_primitives::Vec3;
use auberean_protocol::physics::ClientPosition;
use dereth_primitives::num::math;

/// Running forward, metres a second.
pub const RUN: f32 = 6.5;
/// Backing up.
pub const BACK: f32 = 4.0;
/// Sidestepping.
pub const STRAFE: f32 = 4.5;

/// The least change the report rule counts.
pub const EPSILON: f32 = 0.0002;
/// How long the body may move before the next report, seconds.
pub const REPORT_INTERVAL: f64 = 0.2;

/// The report flag of a jump.
pub const JUMP: u32 = 2;
/// The report flag of a player moving or turning this frame.
pub const MOVING: u32 = 4;

/// What the last report said.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Reported {
    cell: u32,
    origin: Vec3,
    heading: f32,
    motion: [f32; 3],
}

/// The player's body and its reports.
#[derive(Clone, Debug, Default)]
pub struct Body {
    pub cell: u32,
    pub origin: Vec3,
    /// Degrees clockwise from north.
    pub heading: f32,
    pub asked: Motion,
    pub instance_stamp: u16,
    pub teleport_stamp: u16,
    pub force_position_stamp: u16,
    /// The stamp of the last position of the player's own the server sent.
    pub position_stamp: u16,
    /// The stamp of the last move the server ordered the body to make.
    pub move_to_stamp: u16,
    /// Movement frozen by the server's scripts.
    pub frozen: bool,
    /// Moved by the host's own physics ([`Body::follow`]): the body takes no step of its own.
    pub driven: bool,
    /// The host's physics has not yet found room for the body where the server put it: it is out
    /// of the world, and nothing is reported of it.
    pub out_of_world: bool,
    /// Where the body is turning to, when it is turning.
    turning_to: Option<f32>,
    placed: bool,
    placements: u32,
    impulses: u32,
    forced_moves: u32,
    impulse: Vec3,
    times_jumped: u32,
    jumps_reported: u32,
    jump_velocity: Vec3,
    last: Option<Reported>,
    last_time: f64,
}

/// The heading as the report carries it: a byte, 255 to a full turn.
#[must_use]
pub fn heading_byte(degrees: f32) -> u32 {
    let d = degrees.rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (d * 255.0 / 360.0) as u32;
    b.min(255)
}

/// A motion component as the report carries it: -1 to 1 over 0 to 255, none at 127.
#[must_use]
pub fn motion_byte(v: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = ((v.clamp(-1.0, 1.0) + 1.0) * 127.5) as u32;
    b.min(255)
}

/// Whether stamp `b` is newer than stamp `a`, the stamps counting round through 65536.
// D2-05665
#[must_use]
pub fn is_newer(a: u16, b: u16) -> bool {
    a != b && b.wrapping_sub(a) < 0x8000
}

impl Body {
    /// The server put the body somewhere.
    pub fn place(&mut self, cell: u32, origin: Vec3, heading: f32) {
        self.cell = cell;
        self.origin = origin;
        self.heading = heading;
        self.placed = true;
        self.placements = self.placements.wrapping_add(1);
    }

    /// How many times the server has put the body somewhere: a host that moves the body with its
    /// own physics puts its body there whenever this changes.
    #[must_use]
    pub fn placements(&self) -> u32 {
        self.placements
    }

    /// The server threw the body: it leaves the ground at this velocity.
    pub fn throw(&mut self, velocity: Vec3) {
        self.impulse = velocity;
        self.impulses = self.impulses.wrapping_add(1);
    }

    /// How many times the server has thrown the body, and the last velocity: a host that moves the
    /// body with its own physics applies it whenever the count changes.
    #[must_use]
    pub fn thrown(&self) -> (u32, Vec3) {
        (self.impulses, self.impulse)
    }

    /// The server forced the body's place with a newer stamp.
    pub fn force_moved(&mut self) {
        self.forced_moves = self.forced_moves.wrapping_add(1);
    }

    /// How many times the server has forced the body's place: a host with a camera turns it on
    /// whenever this changes.
    #[must_use]
    pub fn forced_moves(&self) -> u32 {
        self.forced_moves
    }

    /// Where the host's physics has the body now (its cell, its place in the cell's landblock and
    /// its heading): what the next report carries.
    pub fn follow(&mut self, cell: u32, origin: Vec3, heading: f32) {
        self.cell = cell;
        self.origin = origin;
        self.heading = heading;
    }

    /// Where the host's physics has the body turning to, if it is: the next report says that in
    /// place of where it faces now.
    // D2-07037
    pub fn turning_to(&mut self, heading: Option<f32>) {
        self.turning_to = heading;
    }

    /// Whether the body has a place yet.
    #[must_use]
    pub fn placed(&self) -> bool {
        self.placed
    }

    /// The player asks for a motion and a facing.
    pub fn ask(&mut self, m: Motion) {
        self.asked = m;
        self.heading = m.heading;
    }

    /// The player jumps.
    pub fn jump(&mut self, velocity: Vec3) {
        self.times_jumped += 1;
        self.jump_velocity = velocity;
    }

    /// Move the body for `dt` seconds as asked.
    pub fn step(&mut self, dt: f32) {
        if self.frozen || self.driven {
            return;
        }
        let a = self.asked;
        let forward = if a.forward >= 0.0 {
            a.forward * RUN
        } else {
            a.forward * BACK
        };
        let sideways = a.sideways * STRAFE;
        if forward == 0.0 && sideways == 0.0 {
            return;
        }
        let (s, c) = math::sin_cosf(self.heading.to_radians());
        // Heading clockwise from north: north is +y, east is +x.
        self.origin.x += (forward * s + sideways * c) * dt;
        self.origin.y += (forward * c - sideways * s) * dt;
    }

    fn motion(&self) -> [f32; 3] {
        [self.asked.sideways, self.asked.forward, 0.0]
    }

    /// Whether the client sends a report this frame.
    // D2-09635, D2-09636
    #[must_use]
    pub fn should_report(&self, now: f64) -> bool {
        if !self.placed || self.out_of_world {
            return false;
        }
        if self.times_jumped > self.jumps_reported {
            return true;
        }
        let Some(last) = self.last else { return true };
        let m = self.motion();
        if m.iter()
            .zip(last.motion)
            .any(|(a, b)| (a - b).abs() >= EPSILON)
        {
            return true;
        }
        if now - self.last_time <= REPORT_INTERVAL {
            return false;
        }
        let moved = self.cell != last.cell
            || (self.origin.x - last.origin.x).abs() >= EPSILON
            || (self.origin.y - last.origin.y).abs() >= EPSILON
            || (self.origin.z - last.origin.z).abs() >= EPSILON;
        moved || !Heading::new(self.heading).equals(Heading::new(last.heading))
    }

    /// The report, as sent now; the body remembers it.
    // D2-15433, D2-09636, D2-13126
    pub fn report(&mut self, now: f64, game_time: f64) -> ClientPosition {
        let m = self.motion();
        let heading = self.turning_to.unwrap_or(self.heading);
        let mut flags = 0;
        if self.asked.active {
            flags |= MOVING;
        }
        let mut jump_velocity = Vec3::default();
        if self.times_jumped > self.jumps_reported {
            flags |= JUMP;
            jump_velocity = self.jump_velocity;
            self.jumps_reported += 1;
        }
        self.last = Some(Reported {
            cell: self.cell,
            origin: self.origin,
            heading,
            motion: m,
        });
        self.last_time = now;
        ClientPosition {
            time: game_time,
            cell: self.cell,
            origin: self.origin,
            heading_motion: heading_byte(heading) << 24
                | motion_byte(m[0]) << 16
                | motion_byte(m[1]) << 8
                | motion_byte(m[2]),
            flags,
            instance_stamp: self.instance_stamp,
            teleport_stamp: self.teleport_stamp,
            force_position_stamp: self.force_position_stamp,
            move_to_stamp: self.move_to_stamp,
            jump_velocity,
        }
    }
}
