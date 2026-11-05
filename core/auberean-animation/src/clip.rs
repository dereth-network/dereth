//! Keyframe clips: per bone, sparse keys of scale, offset, rotation and weight, with times in
//! seconds, parent-relative like the rest pose.
//!
//! A clip is played at a phase in cycles: the fraction of the phase times the clip's period is the
//! time in seconds. Each track takes the last key at or before that time and, when another follows,
//! interpolates toward it (linearly for vectors and weights, spherically for rotations); past the
//! last key it holds it, so a clip loops seamlessly only when its last key repeats its first. A
//! played clip adds its bones, scaled by their weight, into an accumulating pose.

use auberean_assets::animation::{BoneTrack, KeyframeAnimation, RotationKey, VectorKey, WeightKey};
use auberean_primitives::{Quat, Vec3};
use dereth_primitives::num::math;

use crate::lookup::LookupTable;
use crate::math::{lerp, quat_dot};

/// One bone of an accumulating pose: weighted sums of offsets, scales and rotations (the
/// rotation's components summed as they are).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sum {
    pub offset: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
}

impl Sum {
    pub const ZERO: Self = Self {
        offset: Vec3::ZERO,
        scale: Vec3::ZERO,
        rotation: Quat {
            w: 0.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    };

    /// Add `w` times a bone transform.
    // D2-13263
    pub fn add(&mut self, offset: Vec3, scale: Vec3, rotation: Quat, w: f32) {
        self.offset = add_scaled(self.offset, offset, w);
        self.scale = add_scaled(self.scale, scale, w);
        self.rotation = Quat {
            w: self.rotation.w + rotation.w * w,
            x: self.rotation.x + rotation.x * w,
            y: self.rotation.y + rotation.y * w,
            z: self.rotation.z + rotation.z * w,
        };
    }
}

fn add_scaled(a: Vec3, b: Vec3, w: f32) -> Vec3 {
    Vec3::new(a.x + b.x * w, a.y + b.y * w, a.z + b.z * w)
}

/// An accumulating pose: per bone, its weighted sum and its total weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub bones: Vec<Sum>,
    pub weights: Vec<f32>,
}

impl Pose {
    /// An empty pose of `n` bones.
    #[must_use]
    pub fn new(n: usize) -> Self {
        Self {
            bones: vec![Sum::ZERO; n],
            weights: vec![0.0; n],
        }
    }

    /// Clear every bone.
    pub fn clear(&mut self) {
        self.bones.fill(Sum::ZERO);
        self.weights.fill(0.0);
    }
}

/// The spherical interpolation the client uses: the short way round, linear when the two
/// rotations are within about a third of a degree of each other, and not scaled back to unit
/// length.
// D2-17136
#[must_use]
pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let mut cos = quat_dot(a, b);
    let sign = if cos < 0.0 {
        cos = -cos;
        -1.0
    } else {
        1.0
    };
    let (wa, wb) = if cos > 0.999_98 {
        (1.0 - t, t)
    } else {
        let angle = math::acosf(cos);
        let s = math::sinf(angle);
        (math::sinf((1.0 - t) * angle) / s, math::sinf(t * angle) / s)
    };
    let wb = wb * sign;
    Quat {
        w: a.w * wa + b.w * wb,
        x: a.x * wa + b.x * wb,
        y: a.y * wa + b.y * wb,
        z: a.z * wa + b.z * wb,
    }
}

/// The key at or before time `t` and the fraction of the way to the next one (0 past the last),
/// found through the track's lookup table as the client finds it (a plain search when the track
/// has no table: its first key is not at 0).
fn bracket<K>(
    keys: &[K],
    table: Option<&LookupTable>,
    t: f32,
    time: impl Fn(&K) -> f32,
) -> Option<(usize, f32)> {
    if keys.is_empty() {
        return None;
    }
    let at = |k: usize| keys.get(k).map_or(f32::INFINITY, &time);
    let found = |table: &LookupTable| {
        let k = table.find_by(at, t);
        if k == u32::MAX {
            0
        } else {
            (k as usize).min(keys.len() - 1)
        }
    };
    let i = match table {
        Some(table) => found(table),
        None => {
            let times: Vec<f32> = keys.iter().map(&time).collect();
            match LookupTable::new(&times) {
                Some(table) => found(&table),
                None => times.partition_point(|&x| x <= t).saturating_sub(1),
            }
        }
    };
    if i + 1 >= keys.len() {
        return Some((i, 0.0));
    }
    let (t0, t1) = (at(i), at(i + 1));
    let span = t1 - t0;
    Some((i, if span > 0.0 { (t - t0) / span } else { 0.0 }))
}

fn vector_at(keys: &[VectorKey], table: Option<&LookupTable>, t: f32, none: Vec3) -> Vec3 {
    match bracket(keys, table, t, |k| k.time) {
        None => none,
        Some((i, 0.0)) => keys[i].value,
        Some((i, u)) => lerp(keys[i].value, keys[i + 1].value, u),
    }
}

fn rotation_at(keys: &[RotationKey], table: Option<&LookupTable>, t: f32) -> Quat {
    match bracket(keys, table, t, |k| k.time) {
        None => Quat::IDENTITY,
        Some((i, 0.0)) => keys[i].value,
        Some((i, u)) => slerp(keys[i].value, keys[i + 1].value, u),
    }
}

fn weight_at(keys: &[WeightKey], table: Option<&LookupTable>, t: f32) -> f32 {
    match bracket(keys, table, t, |k| k.time) {
        None => 1.0,
        Some((i, 0.0)) => keys[i].weight,
        Some((i, u)) => keys[i].weight + (keys[i + 1].weight - keys[i].weight) * u,
    }
}

/// One bone's transform and weight at time `t` seconds: offset, scale, rotation, weight. A track
/// with no keys gives no offset, unit scale, no rotation and full weight.
#[must_use]
pub fn bone_at(track: &BoneTrack, t: f32) -> (Vec3, Vec3, Quat, f32) {
    let [scale, offset, rotation, weight] = track.tables.each_ref().map(Option::as_ref);
    (
        vector_at(&track.offset, offset, t, Vec3::ZERO),
        vector_at(&track.scale, scale, t, Vec3::new(1.0, 1.0, 1.0)),
        rotation_at(&track.rotation, rotation, t),
        weight_at(&track.weight, weight, t),
    )
}

/// A keyframe clip ready to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip<'a> {
    pub keys: &'a KeyframeAnimation,
}

impl Clip<'_> {
    /// Add the clip at `phase` cycles, scaled by `weight`, into `out`. `parents` says which bones
    /// are roots: a root bone's rotation is turned to the same side as what it is added to.
    // D2-15261, D2-15247, D2-15248, D2-15249
    pub fn animate(&self, weight: f32, phase: f32, parents: &[Option<usize>], out: &mut Pose) {
        if weight == 0.0 {
            return;
        }
        let t = ((phase - phase.floor()) * self.keys.period).max(0.0);
        for (b, track) in self.keys.bones.iter().enumerate().take(out.bones.len()) {
            let (offset, scale, mut rotation, wk) = bone_at(track, t);
            let w = wk * weight;
            if parents.get(b).copied().flatten().is_none()
                && quat_dot(rotation, out.bones[b].rotation) < 0.0
            {
                rotation = Quat {
                    w: -rotation.w,
                    x: -rotation.x,
                    y: -rotation.y,
                    z: -rotation.z,
                };
            }
            out.bones[b].add(offset, scale, rotation, w);
            out.weights[b] += w;
        }
    }
}
