//! The free camera: where it is in the world, which way it looks, how fast it flies, and the
//! snap to the ground.
//!
//! The position is in world meters. The yaw turns counter-clockwise about up from north, the
//! pitch looks up when positive.

use dereth_primitives::num::math;
use dereth_primitives::Vec3;

/// The flying speeds, in meters a second, from slowest.
pub const SPEEDS: [f32; 8] = [1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0];

/// How fast a body runs at run rate 1, in meters a second: the speed a walking body is given is
/// the camera's over this.
pub const RUN_SPEED: f32 = 4.0;

/// The run rates a walking body is held between.
pub const RUN_RATES: std::ops::RangeInclusive<f32> = 0.25..=12.0;

/// How high over the ground the snap puts the camera, in meters.
pub const SNAP_HEIGHT: f32 = 2.0;

/// The pitch's limit either way, short of straight up or down.
pub const PITCH_LIMIT: f32 = 1.55;

/// How far the camera turns for one pixel of the pointer, in radians.
pub const LOOK_PER_PIXEL: f32 = 0.004;

/// The movement keys held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

/// The editor's free camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlyCamera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// An index into [`SPEEDS`].
    pub speed: usize,
}

impl FlyCamera {
    #[must_use]
    pub fn new(position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT),
            speed: 3,
        }
    }

    /// The flying speed, in meters a second.
    #[must_use]
    pub fn meters_per_second(&self) -> f32 {
        SPEEDS[self.speed.min(SPEEDS.len() - 1)]
    }

    /// The run rate a walking body moves at this speed.
    #[must_use]
    pub fn run_rate(&self) -> f32 {
        (self.meters_per_second() / RUN_SPEED).clamp(*RUN_RATES.start(), *RUN_RATES.end())
    }

    /// One speed faster (`steps` > 0) or slower.
    pub fn change_speed(&mut self, steps: i32) {
        let i = i64::try_from(self.speed).unwrap_or(0) + i64::from(steps);
        self.speed = usize::try_from(i.clamp(0, SPEEDS.len() as i64 - 1)).unwrap_or(0);
    }

    /// The way the camera looks.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = (math::sinf(self.yaw), math::cosf(self.yaw));
        let (sp, cp) = (math::sinf(self.pitch), math::cosf(self.pitch));
        Vec3::new(-sy * cp, cy * cp, sp)
    }

    /// The camera's right, level.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        let (sy, cy) = (math::sinf(self.yaw), math::cosf(self.yaw));
        Vec3::new(cy, sy, 0.0)
    }

    /// Fly for `dt` seconds with `held` keys down.
    pub fn fly(&mut self, held: Held, dt: f32) {
        let f = self.forward();
        let r = self.right();
        let axis = |a: bool, b: bool| f32::from(u8::from(a)) - f32::from(u8::from(b));
        let (ahead, side, lift) = (
            axis(held.forward, held.back),
            axis(held.right, held.left),
            axis(held.up, held.down),
        );
        let speed = self.meters_per_second() * dt;
        self.position = Vec3::new(
            self.position.x + (f.x * ahead + r.x * side) * speed,
            self.position.y + (f.y * ahead + r.y * side) * speed,
            self.position.z + (f.z * ahead + lift) * speed,
        );
    }

    /// Turn by the pointer's motion `(dx, dy)` in pixels: right turns right, down looks down.
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * LOOK_PER_PIXEL;
        self.pitch = (self.pitch - dy * LOOK_PER_PIXEL).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Put the camera [`SNAP_HEIGHT`] over `ground`, the ground's height under it.
    pub fn snap(&mut self, ground: f32) {
        self.position.z = ground + SNAP_HEIGHT;
    }
}

/// The speed indoors starts at, in dungeons and rooms: 10 m/s.
pub const INDOOR_SPEED: usize = 3;

/// The speed outdoors starts at: 100 m/s.
pub const OUTDOOR_SPEED: usize = 6;

/// The two flying speeds, each an index into [`SPEEDS`]: one indoors (in a dungeon or a room)
/// and one outdoors, each keeping its own change. The camera flies at the one of where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Speeds {
    pub indoors: usize,
    pub outdoors: usize,
    /// Where the camera was last: indoors or not, `None` before the first frame.
    at: Option<bool>,
}

impl Default for Speeds {
    fn default() -> Self {
        Self {
            indoors: INDOOR_SPEED,
            outdoors: OUTDOOR_SPEED,
            at: None,
        }
    }
}

impl Speeds {
    /// Speeds kept from before, taken at the next frame.
    #[must_use]
    pub fn kept(indoors: usize, outdoors: usize) -> Self {
        let last = SPEEDS.len() - 1;
        Self {
            indoors: indoors.min(last),
            outdoors: outdoors.min(last),
            at: None,
        }
    }

    /// The camera now indoors or not: it flies at the speed of where it is. Answers whether its
    /// speed changed.
    pub fn arrive(&mut self, camera: &mut FlyCamera, indoors: bool) -> bool {
        self.at = Some(indoors);
        let speed = *self.of_mut(indoors);
        let changed = camera.speed != speed;
        camera.speed = speed;
        changed
    }

    /// The speed of where the camera is now (outdoors before it has been anywhere) made `step`,
    /// and the camera's.
    pub fn set(&mut self, camera: &mut FlyCamera, step: usize) {
        let step = step.min(SPEEDS.len() - 1);
        *self.of_mut(self.at.unwrap_or(false)) = step;
        camera.speed = step;
    }

    fn of_mut(&mut self, indoors: bool) -> &mut usize {
        if indoors {
            &mut self.indoors
        } else {
            &mut self.outdoors
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a world builder's free camera)
    use super::*;

    #[test]
    fn an_unturned_camera_looks_north_and_its_right_is_east() {
        let c = FlyCamera::new(Vec3::ZERO, 0.0, 0.0);
        let f = c.forward();
        assert!((f.y - 1.0).abs() < 1e-6 && f.x.abs() < 1e-6);
        assert!((c.right().x - 1.0).abs() < 1e-6);
        // A quarter turn counter-clockwise looks west.
        let w = FlyCamera::new(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 0.0).forward();
        assert!((w.x + 1.0).abs() < 1e-6, "{w:?}");
    }

    #[test]
    fn flying_moves_by_the_speed_and_up_and_down_lift_it() {
        let mut c = FlyCamera::new(Vec3::ZERO, 0.0, 0.0);
        c.speed = 3; // 10 m/s
        c.fly(
            Held {
                forward: true,
                ..Held::default()
            },
            0.5,
        );
        assert!((c.position.y - 5.0).abs() < 1e-5, "{:?}", c.position);
        c.fly(
            Held {
                right: true,
                down: true,
                ..Held::default()
            },
            0.25,
        );
        assert!((c.position.x - 2.5).abs() < 1e-5, "{:?}", c.position);
        assert!((c.position.z + 2.5).abs() < 1e-5, "{:?}", c.position);
        assert!((c.run_rate() - 2.5).abs() < 1e-6, "10 m/s runs at 2.5");
        c.change_speed(10);
        assert_eq!(c.meters_per_second(), 250.0);
        assert_eq!(c.run_rate(), 12.0, "held at the fastest rate");
        c.change_speed(-20);
        assert_eq!(c.meters_per_second(), 1.0);
    }

    #[test]
    fn indoors_and_outdoors_each_keep_their_own_speed() {
        let mut s = Speeds::default();
        let mut c = FlyCamera::new(Vec3::ZERO, 0.0, 0.0);
        assert!(s.arrive(&mut c, false));
        assert_eq!(c.meters_per_second(), 100.0);
        assert!(!s.arrive(&mut c, false), "still outdoors");
        s.set(&mut c, 5);
        assert!(s.arrive(&mut c, true));
        assert_eq!(c.meters_per_second(), 10.0);
        s.set(&mut c, 4);
        // A camera put back as it was before, with the speed it had then, takes the one kept.
        c.speed = 6;
        s.arrive(&mut c, false);
        assert_eq!(c.meters_per_second(), 50.0, "outdoors as it was left");
        c.speed = 0;
        s.arrive(&mut c, true);
        assert_eq!(c.meters_per_second(), 25.0, "indoors as it was left");
        assert_eq!((s.indoors, s.outdoors), (4, 5));
        s.set(&mut c, 99);
        assert_eq!(c.meters_per_second(), 250.0, "the fastest");
    }
}
