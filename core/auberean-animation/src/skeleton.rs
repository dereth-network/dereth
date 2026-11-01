//! A body's skeleton: its bones with their parents, and the rest pose its meshes were modeled in.
//!
//! A setup lists its bones as a parent index and a kind, and holds named poses, each a
//! parent-relative offset, scale and rotation per bone. The rest pose (pose kind 1) places the
//! skeleton as the meshes were modeled: a bone's world rest transform is its own rest transform
//! followed by its parent's world rest transform, and skinning moves a vertex by a bone's posed
//! transform after undoing that bone's world rest transform.

use auberean_assets::setup::{BoneTransform, Setup};
use auberean_world::math::Mat4;

use crate::math::{affine_inverse, transform_matrix};

/// The pose kind that holds the rest pose.
pub const REST_POSE: u32 = 1;

/// A skeleton built from a setup.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    /// Each bone's parent, `None` for a root.
    pub parents: Vec<Option<usize>>,
    /// Each bone's kind (which joint it is).
    pub kinds: Vec<u32>,
    /// Each bone's rest transform, relative to its parent.
    pub rest: Vec<BoneTransform>,
    /// Each bone's world rest transform.
    pub rest_world: Vec<Mat4>,
    /// The inverse of each bone's world rest transform.
    pub inverse_rest: Vec<Mat4>,
}

impl Skeleton {
    /// The skeleton of `setup`, or `None` when it has no bones, no rest pose with a transform for
    /// every bone, or a bone whose parent does not come before it.
    // D2-12903
    #[must_use]
    pub fn from_setup(setup: &Setup) -> Option<Self> {
        if setup.bones.is_empty() {
            return None;
        }
        let pose = setup.poses.iter().find(|p| p.kind == REST_POSE)?;
        if pose.bones.len() != setup.bones.len() {
            return None;
        }
        let mut parents = Vec::with_capacity(setup.bones.len());
        for (i, b) in setup.bones.iter().enumerate() {
            let p = usize::try_from(b.parent).ok().filter(|&p| p < i);
            if b.parent != u32::MAX && p.is_none() {
                return None;
            }
            parents.push(p);
        }
        let mut rest_world: Vec<Mat4> = Vec::with_capacity(parents.len());
        for (i, t) in pose.bones.iter().enumerate() {
            let local = transform_matrix(t);
            let world = match parents[i] {
                Some(p) => local.then(&rest_world[p]),
                None => local,
            };
            rest_world.push(world);
        }
        let inverse_rest = rest_world.iter().map(affine_inverse).collect();
        Some(Self {
            parents,
            kinds: setup.bones.iter().map(|b| b.id).collect(),
            rest: pose.bones.clone(),
            rest_world,
            inverse_rest,
        })
    }

    /// How many bones it has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parents.len()
    }

    /// Whether it has no bones.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }

    /// The world transforms of a pose given as parent-relative transforms, one per bone.
    ///
    /// # Panics
    /// When `local` does not hold one transform per bone.
    #[must_use]
    pub fn world(&self, local: &[BoneTransform]) -> Vec<Mat4> {
        assert_eq!(local.len(), self.len(), "one transform per bone");
        let mut out: Vec<Mat4> = Vec::with_capacity(local.len());
        for (i, t) in local.iter().enumerate() {
            let m = transform_matrix(t);
            out.push(match self.parents[i] {
                Some(p) => m.then(&out[p]),
                None => m,
            });
        }
        out
    }

    /// The skinning palette of a pose: per bone, its inverse world rest transform followed by its
    /// posed world transform, so a vertex modeled at rest follows the bone.
    // D2-06604
    #[must_use]
    pub fn palette(&self, world: &[Mat4]) -> Vec<Mat4> {
        self.inverse_rest
            .iter()
            .zip(world)
            .map(|(inv, w)| inv.then(w))
            .collect()
    }
}
