//! Setups (`.set`, `0x04000000`+): how a body's parts are put together. A setup names the meshes
//! a body is drawn with and its skeleton (each bone with its parent), places the skeleton in named
//! poses, says where the body holds things and where things attach to it, gives its collision
//! shapes and sizes, and sets the distances its detail drops at.

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use auberean_primitives::{BBox, DataId, Quat, Sphere, Vec3};

use crate::{Context, Decode};

/// A bone's or a point's transform: offset, scale and rotation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BoneTransform {
    pub offset: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
}

impl BoneTransform {
    /// The offset, the scale, then the rotation.
    // D2-13269
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            offset: r.vec3()?,
            scale: r.vec3()?,
            rotation: r.quat()?,
        })
    }

    /// A frame's place and rotation at a scale of one.
    // D2-13262
    #[must_use]
    pub fn of_frame(offset: Vec3, rotation: Quat) -> Self {
        Self {
            offset,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation,
        }
    }

    /// Every part multiplied by `weight`, to be summed with others and made whole.
    // D2-13264
    #[must_use]
    pub fn weighted(&self, weight: f32) -> Self {
        let v = |v: Vec3| Vec3::new(v.x * weight, v.y * weight, v.z * weight);
        let q = self.rotation;
        Self {
            offset: v(self.offset),
            scale: v(self.scale),
            rotation: Quat {
                w: q.w * weight,
                x: q.x * weight,
                y: q.y * weight,
                z: q.z * weight,
            },
        }
    }

    /// Whether every part is a number and the rotation is a unit, within 0.0002.
    // D2-13268
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let q = self.rotation;
        let parts = [
            self.offset.x,
            self.offset.y,
            self.offset.z,
            self.scale.x,
            self.scale.y,
            self.scale.z,
            q.w,
            q.x,
            q.y,
            q.z,
        ];
        let length = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
        !parts.iter().any(|v| v.is_nan()) && (length - 1.0).abs() < 0.0002
    }
}

/// One bone of the skeleton: the index of its parent bone, and its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Bone {
    // D2-12883, D2-12885, D2-12889
    /// The parent bone's index; `u32::MAX` for the root.
    pub parent: u32,
    pub id: u32,
}

/// A named pose of the skeleton: a transform per bone.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Pose {
    // D2-12880, D2-12879, D2-12881
    /// Which pose this is (resting, sitting...).
    pub kind: u32,
    pub bones: Vec<BoneTransform>,
}

impl Pose {
    /// The kind, a bone count, and a transform per bone.
    // D2-12867
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        let bones = r.list(40, BoneTransform::read)?;
        Ok(Self { kind, bones })
    }
}

/// A place on the body: where it holds something, or where something attaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    // D2-12878
    /// Which place this is (right hand, back...).
    pub kind: u32,
    /// The bone it moves with; `u32::MAX` for none.
    pub bone: u32,
    pub transform: BoneTransform,
}

impl Default for Location {
    /// A place not yet read from a setup: kind 0, on no bone.
    // D2-06174, D2-06175, D2-06176
    fn default() -> Self {
        Self {
            kind: 0,
            bone: u32::MAX,
            transform: BoneTransform::default(),
        }
    }
}

impl Location {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            kind: r.u32()?,
            bone: r.u32()?,
            transform: BoneTransform::read(r)?,
        })
    }
}

/// An upright cylinder with rounded ends, one of the body's collision shapes.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Cylinder {
    /// The center of its base.
    pub low: Vec3,
    pub radius: f32,
    pub height: f32,
}

/// The distances at which one level of the body's detail is seen and degraded.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DetailDistances {
    pub visible: f32,
    pub near_degrade: f32,
    pub ideal_degrade: f32,
    pub far_degrade: f32,
    pub high_detail: f32,
    pub medium_detail: f32,
}

/// A setup (`.set`). Copied whole into another: its meshes, bones, poses, holding and
/// connection places, shapes and measures.
// D2-12901
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Setup {
    // D2-12909, D2-12905, D2-12907, D2-12902
    /// `0x4`: the body may turn freely; `0x8`: it has a physics tree.
    pub flags: u32,
    // D2-12882, D2-12884, D2-12887
    pub meshes: Vec<DataId>,
    pub bones: Vec<Bone>,
    pub poses: Vec<Pose>,
    pub holding_locations: Vec<Location>,
    pub connection_points: Vec<Location>,
    pub cylinders: Vec<Cylinder>,
    pub spheres: Vec<Sphere>,
    pub height: f32,
    pub radius: f32,
    pub step_up_height: f32,
    pub step_down_height: f32,
    pub bounding_sphere: Sphere,
    pub selection_sphere: Sphere,
    /// Three levels of detail.
    pub detail: [DetailDistances; 3],
    /// How the body casts its shadow.
    pub shadow: u32,
    pub physics_radius: f32,
    pub physics_bounds: BBox,
}

impl Setup {
    /// Whether the body may turn freely.
    #[must_use]
    pub fn turns_freely(&self) -> bool {
        self.flags & 0x4 != 0
    }

    /// Whether the body has a physics tree.
    #[must_use]
    pub fn has_physics_tree(&self) -> bool {
        self.flags & 0x8 != 0
    }
}

impl Decode for Setup {
    /// The id; the flags; a counted list of mesh ids; a counted list of bones (parent index, id);
    /// a counted list of poses; a counted list of holding locations and one of connection points
    /// (each a kind, a bone and a transform); a counted list of cylinders (base, radius, height);
    /// a counted list of spheres; the height, radius, step-up and step-down heights; the bounding
    /// and selection spheres; three sets of detail distances; the shadow kind; the physics radius;
    /// the physics bounding box.
    // D2-12900, D2-12693, D2-13153, D2-13135, D2-12869, D2-13224, D2-03466, D2-03467
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let flags = r.u32()?;
        // D2-12886
        let meshes = r.list(4, Reader::data_id)?;
        // D2-12888
        let bones = r.list(8, |r| {
            Ok(Bone {
                parent: r.u32()?,
                id: r.u32()?,
            })
        })?;
        let poses = r.list(8, Pose::read)?;
        let holding_locations = r.list(48, Location::read)?;
        let connection_points = r.list(48, Location::read)?;
        let cylinders = r.list(20, |r| {
            Ok(Cylinder {
                low: r.vec3()?,
                radius: r.f32()?,
                height: r.f32()?,
            })
        })?;
        let spheres = r.list(16, Reader::sphere)?;
        let height = r.f32()?;
        let radius = r.f32()?;
        let step_up_height = r.f32()?;
        let step_down_height = r.f32()?;
        let bounding_sphere = r.sphere()?;
        let selection_sphere = r.sphere()?;
        let mut detail = [DetailDistances::default(); 3];
        for d in &mut detail {
            *d = DetailDistances {
                visible: r.f32()?,
                near_degrade: r.f32()?,
                ideal_degrade: r.f32()?,
                far_degrade: r.f32()?,
                high_detail: r.f32()?,
                medium_detail: r.f32()?,
            };
        }
        let shadow = r.u32()?;
        let physics_radius = r.f32()?;
        let physics_bounds = r.bbox()?;
        Ok(Self {
            flags,
            meshes,
            bones,
            poses,
            holding_locations,
            connection_points,
            cylinders,
            spheres,
            height,
            radius,
            step_up_height,
            step_down_height,
            bounding_sphere,
            selection_sphere,
            detail,
            shadow,
            physics_radius,
            physics_bounds,
        })
    }
}

#[cfg(test)]
mod transform_tests {
    //! Behaviour: none (a bone transform's parts, held to hand-worked values)
    use super::*;

    #[test]
    fn a_bone_transform_writes_weighs_and_checks_itself() {
        let unit = Quat {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        let t = BoneTransform::of_frame(Vec3::new(1.0, 2.0, 3.0), unit);
        assert_eq!(t.scale, Vec3::new(1.0, 1.0, 1.0));
        assert!(t.is_valid());
        let half = t.weighted(0.5);
        assert_eq!(half.offset, Vec3::new(0.5, 1.0, 1.5));
        assert_eq!(half.rotation.w, 0.5);
        assert!(!half.is_valid(), "its rotation no longer a unit");
        assert!(!BoneTransform {
            offset: Vec3::new(f32::NAN, 0.0, 0.0),
            ..t
        }
        .is_valid());
    }
}
