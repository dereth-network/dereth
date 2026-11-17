//! In the air: a player is in the air from a jump its client reports until the jump comes back to
//! the height it left from (twice its rise at the jump's upward speed), and for as long after as
//! the places its client reports keep going down (off a ledge), within [`STALE`] seconds of the
//! last of them. A report that goes down no further lands it. No skill is used in the air
//! ([`crate::skill_use`]).

/// How fast a body in the air gains downward speed, in meters a second each second.
pub const GRAVITY: f32 = 9.8;
/// How far a report goes down, in meters, to count as going down.
pub const DOWN: f32 = 0.05;
/// How long after its last report a player still going down is taken as in the air, in seconds.
pub const STALE: f64 = 1.0;

/// A player's time in the air, from the places its client reports.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aloft {
    /// When its last jump comes back down to where it left from (world seconds); none on the
    /// ground.
    lands: Option<f64>,
    /// Its height at its last report, when that report was taken, and whether it had gone down
    /// since the report before, past the jump's own time.
    z: f32,
    at: f64,
    going_down: bool,
}

impl Aloft {
    /// A report taken at `now` (world seconds): its height, and its jump's velocity if it jumped.
    pub fn report(&mut self, now: f64, z: f32, jump: Option<f32>) {
        let before = self.z;
        self.z = z;
        self.at = now;
        if let Some(up) = jump {
            self.lands = Some(now + f64::from(2.0 * up.max(0.0) / GRAVITY));
            self.going_down = false;
            return;
        }
        let Some(lands) = self.lands else { return };
        if now >= lands {
            self.going_down = before - z > DOWN;
            if !self.going_down {
                self.lands = None;
            }
        }
    }

    /// Is the player in the air at `now`?
    #[must_use]
    pub fn in_the_air(&self, now: f64) -> bool {
        self.lands
            .is_some_and(|lands| now < lands || (self.going_down && now - self.at <= STALE))
    }
}
