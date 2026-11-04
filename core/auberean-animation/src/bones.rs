//! Where a body holds things and where its bones are: the holding locations its part groups'
//! setups name, the bone of a kind, and the bones' transforms in the pose the body last took.
//!
//! A question goes to the body's part groups in order, each group before the groups hung from it,
//! and the first group that answers decides. A bone's transform is in its group's frame, without
//! the bones' own scales ([`crate::body::PartPose::bones`]).

use auberean_assets::setup::Location;
use auberean_world::math::Mat4;

use crate::body::{Body, BodyRecords};
use crate::setups::{holding_location, BoneTypes};

impl Body {
    /// Whether some part group's setup names a holding location of `kind`.
    // D2-06638, D2-06583, D2-06450
    #[must_use]
    pub fn has_holding_location(&self, kind: u32) -> bool {
        self.parts
            .iter()
            .any(|p| holding_location(&p.setup, kind).is_some())
    }

    /// The holding location of `kind` of the first part group whose setup names one: the group's
    /// index, a copy of the location, and the transform of the bone it is on in the body's current
    /// pose (the identity for a bone past the group's). None when no group names one.
    // D2-06659, D2-06584, D2-06656, D2-06451
    #[must_use]
    pub fn holding_location(
        &self,
        records: &dyn BodyRecords,
        kind: u32,
    ) -> Option<(usize, Location, Mat4)> {
        let (group, location) = self
            .parts
            .iter()
            .enumerate()
            .find_map(|(i, p)| holding_location(&p.setup, kind).map(|l| (i, *l)))?;
        let pose = self.pose(records);
        let bone = pose
            .get(group)
            .and_then(|p| p.bones.get(location.bone as usize))
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        Some((group, location, bone))
    }

    /// The first bone of `kind` in the first part group that has one: the group's index and the
    /// bone's.
    // D2-06639, D2-06587
    #[must_use]
    pub fn find_bone_type(&self, kind: u32) -> Option<(usize, u32)> {
        self.parts
            .iter()
            .enumerate()
            .find_map(|(i, p)| BoneTypes::new(&p.setup).find(kind).map(|b| (i, b)))
    }

    /// The transform, in the body's current pose, of the first bone of `kind` in the first part
    /// group that has one (the identity for a bone the pose does not reach).
    // D2-06640, D2-06586, D2-06448
    #[must_use]
    pub fn bone_type_matrix(&self, records: &dyn BodyRecords, kind: u32) -> Option<Mat4> {
        let (group, bone) = self.find_bone_type(kind)?;
        Some(
            self.pose_of_bone(records, group, bone as usize)
                .unwrap_or(Mat4::IDENTITY),
        )
    }

    /// The transform, in the body's current pose, of the first part group's first bone, when it
    /// has one.
    // D2-06662
    #[must_use]
    pub fn first_bone_matrix(&self, records: &dyn BodyRecords) -> Option<Mat4> {
        self.pose_of_bone(records, 0, 0)
    }
}
