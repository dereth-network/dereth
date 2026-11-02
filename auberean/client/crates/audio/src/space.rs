//! The positional law: where a placed sound is heard from, how far away, and how it moves.
//!
//! Every placed sound is kept relative to the listener (the camera, standing still: the client
//! gives the listener no velocity): its offset from the listener turned into the listener's own
//! frame and its maker's velocity turned the same way, in the sound device's axes (right, up,
//! ahead). The device then plays it:
//!
//! - **by distance**: at full level out to the setting's minimum distance, and beyond it falling
//!   by 6 decibels for each doubling of the distance (the device's rolloff of 1; the maximum
//!   distance is a billion meters, so it never stops falling);
//! - **by direction**: panned to the side it is on, wholly into one speaker at a right angle to
//!   the listener's view, the height ignored, the same behind as ahead;
//! - **by speed**: its frequency shifted by the speed it closes on the listener at, against a
//!   speed of sound of 343 meters a second (the device's Doppler factor of 1).
//!
//! The listener's settings reach the device once a tenth of a second, each placed sound's at once
//! when it starts.

use dereth_primitives::num::math;

/// The distance the device stops attenuating at, meters.
pub const MAX_DISTANCE: f32 = 1.0e9;

/// The speed of sound, meters a second.
pub const SPEED_OF_SOUND: f32 = 343.0;

/// How often the listener's settings reach the device, seconds.
pub const COMMIT_INTERVAL: f64 = 0.1;

/// The highest frequency the device plays a buffer at, frames a second.
pub const MAX_FREQUENCY: f64 = 200_000.0;

/// A point or direction in the world's axes (meters; x east, y north, z up).
pub type Vec3 = [f32; 3];

/// The listener: where it is and its axes (right, ahead, up) in the world's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub right: Vec3,
    pub ahead: Vec3,
    pub up: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            right: [1.0, 0.0, 0.0],
            ahead: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
        }
    }
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

impl Listener {
    /// A direction in the world turned into the device's axes for this listener: its right, its
    /// up and its ahead parts (the world's up and ahead swap places, as the device's axes have
    /// them).
    #[must_use]
    pub fn turn(&self, v: Vec3) -> Vec3 {
        [dot(v, self.right), dot(v, self.up), dot(v, self.ahead)]
    }

    /// A point in the world relative to the listener, in the device's axes.
    // D2-07772
    #[must_use]
    pub fn local(&self, p: Vec3) -> Vec3 {
        let d = [
            p[0] - self.position[0],
            p[1] - self.position[1],
            p[2] - self.position[2],
        ];
        self.turn(d)
    }
}

/// The gain the distance of a sound at `local` gives it: 1 within `min_distance`, beyond it the
/// minimum distance over the distance.
#[must_use]
pub fn distance_gain(local: Vec3, min_distance: f32) -> f32 {
    let d = length(local).min(MAX_DISTANCE);
    if min_distance <= 0.0 {
        return if d > 0.0 { 0.0 } else { 1.0 };
    }
    if d <= min_distance {
        1.0
    } else {
        min_distance / d
    }
}

/// The left and right gains the direction of a sound at `local` gives it: the side it is on at
/// full level, the other at the cosine of its angle off the line of view (the height ignored, so
/// a sound overhead or at the listener is heard alike in both).
#[must_use]
pub fn pan_gains(local: Vec3) -> [f32; 2] {
    let (x, z) = (local[0], local[2]);
    let flat = math::hypotf(x, z);
    if flat <= f32::EPSILON {
        return [1.0, 1.0];
    }
    // The cosine of the angle off the line of view, ahead and behind alike.
    let far = z.abs() / flat;
    if x >= 0.0 {
        [far, 1.0]
    } else {
        [1.0, far]
    }
}

/// The frequency ratio a sound at `local` moving at `velocity` (both in the device's axes, the
/// listener still) is heard at: the speed of sound over the speed of sound plus the speed it moves
/// away from the listener at. A sound at the listener is not shifted.
#[must_use]
pub fn doppler(local: Vec3, velocity: Vec3) -> f32 {
    let d = length(local);
    if d <= f32::EPSILON {
        return 1.0;
    }
    let away = dot(velocity, local) / d;
    // Held short of the speed of sound, where the shift has no meaning.
    let denominator = (SPEED_OF_SOUND + away).max(SPEED_OF_SOUND * 0.01);
    SPEED_OF_SOUND / denominator
}

/// What the device makes of a placed sound: its distance gain, its two direction gains and its
/// frequency ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub distance: f32,
    pub sides: [f32; 2],
    pub doppler: f32,
}

impl Placement {
    /// Nothing changed: a sound heard where the listener is.
    pub const AT_LISTENER: Self = Self {
        distance: 1.0,
        sides: [1.0, 1.0],
        doppler: 1.0,
    };

    /// A placed sound at `local`, moving at `velocity`, with its setting's minimum distance.
    // D2-07753
    #[must_use]
    pub fn of(local: Vec3, velocity: Vec3, min_distance: f32) -> Self {
        Self {
            distance: distance_gain(local, min_distance),
            sides: pan_gains(local),
            doppler: doppler(local, velocity),
        }
    }
}
