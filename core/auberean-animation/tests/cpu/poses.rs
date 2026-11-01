//! A body's pose on a made-up setup: its bone transforms and its skin.
//! Fixture: a two-bone setup, the child a meter up its parent; at rest neither is scaled, and in
//! the pose the body stands in the parent is twice its size.

use std::sync::Arc;

use auberean_animation::{Animations, Body, BodyRecords};
use auberean_assets::animation::{AnimMap, Animation, BehaviorTable};
use auberean_assets::setup::{Bone, BoneTransform, Pose, Setup};
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::look::{Group, Look, LookRecords};

const SETUP: u32 = 0x0400_0001;

struct Records(Arc<Setup>);

impl Animations for Records {
    fn animation(&self, _id: DataId) -> Option<Arc<Animation>> {
        None
    }
}

impl LookRecords for Records {
    fn visual(&self, _id: DataId) -> Option<Arc<VisualDesc>> {
        None
    }
    fn appearance(&self, _id: DataId) -> Option<Arc<Appearance>> {
        None
    }
}

impl BodyRecords for Records {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>> {
        (id.0 == SETUP).then(|| self.0.clone())
    }
    fn anim_map(&self, _id: DataId) -> Option<Arc<AnimMap>> {
        None
    }
    fn behavior_table(&self, _id: DataId) -> Option<Arc<BehaviorTable>> {
        None
    }
}

fn placed(z: f32, scale: f32) -> BoneTransform {
    BoneTransform {
        offset: Vec3::new(0.0, 0.0, z),
        scale: Vec3::new(scale, scale, scale),
        rotation: Quat::IDENTITY,
    }
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5
}

/// A bone's scale reaches the skin of the bones under it but not their bone transforms: with the
/// parent posed at twice its size, the child's bone transform still stands a meter up, while a
/// vertex modeled on the child at rest, a meter and a half up, is skinned to three meters up (its
/// half meter on the child and the child's meter from the parent both doubled).
///
/// Behaviour: animation.skin.a-bones-scale-reaches-the-skin-under-it
#[test]
fn a_bones_scale_reaches_the_skin_under_it() {
    let setup = Setup {
        bones: vec![
            Bone {
                parent: u32::MAX,
                id: 1,
            },
            Bone { parent: 0, id: 2 },
        ],
        poses: vec![
            Pose {
                kind: 0,
                bones: vec![placed(0.0, 2.0), placed(1.0, 1.0)],
            },
            Pose {
                kind: 1,
                bones: vec![placed(0.0, 1.0), placed(1.0, 1.0)],
            },
        ],
        ..Setup::default()
    };
    let records = Records(Arc::new(setup));
    let mut look = Look::default();
    look.add_group(&Group {
        key: 1,
        setup: DataId(SETUP),
        ..Group::default()
    })
    .expect("the group");
    let body = Body::from_look(&records, &look).expect("the body");
    let pose = &body.pose(&records)[0];
    assert!(
        near(pose.bones[1].point(Vec3::ZERO), Vec3::new(0.0, 0.0, 1.0)),
        "the child's bone takes no scale: {:?}",
        pose.bones[1]
    );
    let skinned = pose.palette[1].point(Vec3::new(0.0, 0.0, 1.5));
    assert!(near(skinned, Vec3::new(0.0, 0.0, 3.0)), "{skinned:?}");
    let root = pose.palette[0].point(Vec3::new(0.0, 0.0, 0.5));
    assert!(near(root, Vec3::new(0.0, 0.0, 1.0)), "{root:?}");
}
