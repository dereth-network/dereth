//! The free camera: where it is in the world, which way it looks, how fast it flies, and the
//! snap to the ground. The camera itself is the world builders' shared one
//! (`dereth_builder::camera`); here are the window's keys that fly it and the runtime's camera
//! made from it.
//!
//! The position is in world meters (`landblock x * 192 + local x`); the runtime's own free camera
//! is relative to the block the drawn window is centered on, and [`RuntimeCamera::to_free`] makes it.
//! The yaw turns counter-clockwise about up from north, the pitch looks up when positive, as the
//! runtime's camera has them.

use dereth_client_runtime::camera::FreeCamera;
use dereth_primitives::Vec3;

pub use dereth_builder::camera::{
    FlyCamera, Held, Speeds, INDOOR_SPEED, LOOK_PER_PIXEL, OUTDOOR_SPEED, PITCH_LIMIT, RUN_RATES,
    RUN_SPEED, SNAP_HEIGHT, SPEEDS,
};

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

/// The editor's free camera made the runtime's, and back.
pub trait RuntimeCamera {
    /// The runtime's free camera for this one, relative to the block `(bx, by)` the window is
    /// centered on.
    fn to_free(&self, block: (i32, i32)) -> FreeCamera;

    /// This camera from the runtime's, relative to the block `(bx, by)`.
    fn from_free(c: &FreeCamera, block: (i32, i32), speed: usize) -> Self;
}

impl RuntimeCamera for FlyCamera {
    fn to_free(&self, block: (i32, i32)) -> FreeCamera {
        let (ox, oy) = block_meters(block);
        FreeCamera::new(
            Vec3::new(self.position.x - ox, self.position.y - oy, self.position.z),
            self.yaw,
            self.pitch,
        )
    }

    fn from_free(c: &FreeCamera, block: (i32, i32), speed: usize) -> Self {
        let (ox, oy) = block_meters(block);
        Self {
            position: Vec3::new(c.position.x + ox, c.position.y + oy, c.position.z),
            yaw: c.yaw,
            pitch: c.pitch,
            speed,
        }
    }
}

/// A block's south-west corner, in world meters.
#[must_use]
pub fn block_meters(block: (i32, i32)) -> (f32, f32) {
    lyceum_foundry::brush::WORLD.block_origin(block)
}

/// The landblock and its meters under world meters `p`.
#[must_use]
pub fn landblock_of(p: Vec3) -> Option<(u16, f32, f32)> {
    lyceum_foundry::brush::WORLD.landblock_at(p.x, p.y)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's free camera)
    use super::*;

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
