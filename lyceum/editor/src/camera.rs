//! The free camera: where it is in the world, which way it looks, how fast it flies, and the
//! snap to the ground.
//!
//! The position is in world meters (`landblock x * 192 + local x`); the runtime's own free camera
//! is relative to the block the drawn window is centered on, and [`FlyCamera::to_free`] makes it.
//! The yaw turns counter-clockwise about up from north, the pitch looks up when positive, as the
//! runtime's camera has them.

use dereth_client_runtime::camera::FreeCamera;
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

/// The flying camera's keys as the window's events leave them: the movement keys held, Shift
/// (which lowers the camera), a click made while Shift was held (the rest of that hold does not
/// lower it), and Q (which lowers it too).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlyKeys {
    pub held: Held,
    pub shift: bool,
    pub shift_clicked: bool,
    pub q: bool,
}

impl FlyKeys {
    /// Key `vk` (a window's virtual key) pressed or let go, with Ctrl held or not. While a text
    /// field takes the typing (`typing`), a key pressed is the field's; a key let go always lets
    /// go. Answers whether the key is the world's.
    pub fn key(&mut self, vk: u32, pressed: bool, typing: bool, ctrl: bool) -> bool {
        if typing && pressed {
            return false;
        }
        match vk {
            0x10 | 0xA0 | 0xA1 => {
                self.shift = pressed;
                if !pressed {
                    self.shift_clicked = false;
                }
            }
            0x57 => self.held.forward = pressed,
            0x53 if !ctrl => self.held.back = pressed,
            0x41 => self.held.left = pressed,
            0x44 if !ctrl => self.held.right = pressed,
            // Space rises as E does.
            0x45 | 0x20 => self.held.up = pressed,
            0x51 => self.q = pressed,
            _ => {}
        }
        true
    }

    /// A click in the world: with Shift held, it adds to the selection, and the rest of that hold
    /// does not lower the camera.
    pub fn clicked(&mut self) {
        if self.shift {
            self.shift_clicked = true;
        }
    }

    /// The keys the camera flies by: Q or Shift lowers it, but not while Shift places a building
    /// freely (`freely`).
    #[must_use]
    pub fn held(&self, freely: bool) -> Held {
        Held {
            down: self.q || self.shift && !self.shift_clicked && !freely,
            ..self.held
        }
    }
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

    /// The runtime's free camera for this one, relative to the block `(bx, by)` the window is
    /// centered on.
    #[must_use]
    pub fn to_free(&self, block: (i32, i32)) -> FreeCamera {
        let (ox, oy) = block_meters(block);
        FreeCamera::new(
            Vec3::new(self.position.x - ox, self.position.y - oy, self.position.z),
            self.yaw,
            self.pitch,
        )
    }

    /// This camera from the runtime's, relative to the block `(bx, by)`.
    #[must_use]
    pub fn from_free(c: &FreeCamera, block: (i32, i32), speed: usize) -> Self {
        let (ox, oy) = block_meters(block);
        Self {
            position: Vec3::new(c.position.x + ox, c.position.y + oy, c.position.z),
            yaw: c.yaw,
            pitch: c.pitch,
            speed,
        }
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

/// A block's south-west corner, in world meters.
#[must_use]
pub fn block_meters((bx, by): (i32, i32)) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)] // a block, 0 to 254
    (bx as f32 * 192.0, by as f32 * 192.0)
}

/// The landblock and its meters under world meters `p`.
#[must_use]
pub fn landblock_of(p: Vec3) -> Option<(u16, f32, f32)> {
    let bx = dereth_primitives::num::floor_to_i32(p.x / 192.0);
    let by = dereth_primitives::num::floor_to_i32(p.y / 192.0);
    if !(0..0xFF).contains(&bx) || !(0..0xFF).contains(&by) {
        return None;
    }
    let lb = u16::try_from((bx << 8) | by).ok()?;
    let (ox, oy) = block_meters((bx, by));
    Some((lb, p.x - ox, p.y - oy))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's free camera)
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

    #[test]
    fn keys_typed_into_a_field_neither_fly_nor_lower_the_camera_and_a_key_let_go_lets_go() {
        let mut k = FlyKeys::default();
        // Typing: Shift (a capital), W, Q, E and Space are the field's.
        for vk in [0x10, 0xA0, 0x57, 0x51, 0x45, 0x20, 0x41, 0x44, 0x53] {
            assert!(!k.key(vk, true, true, false));
        }
        assert_eq!(k.held(false), Held::default());
        // Not typing: Shift lowers the camera, W flies it.
        assert!(k.key(0x10, true, false, false));
        assert!(k.key(0x57, true, false, false));
        let h = k.held(false);
        assert!(h.down && h.forward, "{h:?}");
        // Let go while typing: let go all the same.
        k.key(0x10, false, true, false);
        k.key(0x57, false, true, false);
        assert_eq!(k.held(false), Held::default());
    }

    #[test]
    fn a_click_with_shift_held_stops_it_lowering_the_camera_till_it_is_let_go() {
        let mut k = FlyKeys::default();
        k.key(0x10, true, false, false);
        assert!(k.held(false).down);
        assert!(!k.held(true).down, "placing a building freely");
        k.clicked();
        assert!(!k.held(false).down);
        k.key(0x10, false, false, false);
        k.key(0x10, true, false, false);
        assert!(k.held(false).down);
        // Ctrl+S and Ctrl+D are the editor's, not a step back or right.
        k.key(0x53, true, false, true);
        k.key(0x44, true, false, true);
        assert!(!k.held(false).back && !k.held(false).right);
    }

    #[test]
    fn the_runtime_camera_is_relative_to_the_windows_block() {
        let c = FlyCamera::new(
            Vec3::new(0xA9 as f32 * 192.0 + 10.0, 0xB4 as f32 * 192.0 + 20.0, 80.0),
            0.3,
            -0.2,
        );
        let f = c.to_free((0xA9, 0xB4));
        assert_eq!(f.position, Vec3::new(10.0, 20.0, 80.0));
        assert_eq!(FlyCamera::from_free(&f, (0xA9, 0xB4), c.speed), c);
        assert_eq!(landblock_of(c.position), Some((0xA9B4, 10.0, 20.0)));
    }
}
