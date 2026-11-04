//! What a body's draws keep from frame to frame: the mesh's own list always, and the list of places
//! and facings for as long as the pose stands.
//!
//! Behaviour: none (how the lists are kept, which no behavior of the game names)

use std::sync::Arc;

use auberean_animation::body::Body as Figure;
use auberean_animation::skin::{influences, skin};
use auberean_animation::CachedRecords;
use auberean_assets::mesh::Mesh;
use auberean_draw::body::BodyDrawer;
use auberean_draw::mesh::vertices;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;

use crate::common::install;

const HUMAN_MALE: DataId = DataId(0x1F00_0023);

fn figure() -> (Figure, Mat4) {
    let recs = CachedRecords::new(&install().records);
    let figure = Figure::assemble(&recs, HUMAN_MALE).expect("the human male");
    let place = Mat4::from_parts(
        Vec3::new(10.0, 20.0, 30.0),
        Quat::IDENTITY,
        Vec3::new(1.0, 1.0, 1.0),
    );
    (figure, place)
}

/// A body's places are its mesh's vertices moved by its pose, with the rest of each vertex left to
/// the mesh's own list.
#[test]
fn a_bodys_places_are_its_skinned_vertices() {
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let (figure, place) = figure();
    let poses = figure.pose(&recs);
    let batches = BodyDrawer::default().batches(r, &figure, &poses, &place);
    assert!(!batches.is_empty());
    let mut skinned = 0;
    for b in &batches {
        let posed = b.posed.as_ref().expect("a body's draw has its places");
        assert_eq!(posed.len(), b.vertices.len());
        let mesh = r.get::<Mesh>(b.mesh).expect("its mesh");
        let Some(inf) = influences(&mesh.vertices) else {
            continue;
        };
        skinned += 1;
        let stored = vertices(&mesh.vertices).expect("its vertices");
        assert_eq!(
            stored, *b.vertices,
            "the mesh's own list, as the mesh has it"
        );
        let v3 = |p: [f32; 3]| Vec3::new(p[0], p[1], p[2]);
        let pos: Vec<Vec3> = stored.iter().map(|v| v3(v.position)).collect();
        let nrm: Vec<Vec3> = stored.iter().map(|v| v3(v.normal)).collect();
        let pose = poses
            .iter()
            .find(|pose| {
                let (p, _) = skin(&pos, &nrm, &inf, &pose.palette);
                let at = pose.world.point(p[0]);
                posed[0].position == [at.x, at.y, at.z]
            })
            .expect("a pose of the body moves it there");
        let (p, _) = skin(&pos, &nrm, &inf, &pose.palette);
        for (at, want) in posed.iter().zip(&p) {
            let want = pose.world.point(*want);
            assert_eq!(at.position, [want.x, want.y, want.z]);
        }
    }
    assert!(skinned > 3, "{skinned} skinned draws");
}

/// A pose that stands keeps its lists from frame to frame; a changed pose has new places and the
/// same mesh's list; a pose let go for two frames is not kept.
#[test]
fn a_standing_pose_keeps_its_lists() {
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let (figure, place) = figure();
    let poses = figure.pose(&recs);
    let mut drawer = BodyDrawer::default();
    let first = drawer.batches(r, &figure, &poses, &place);
    drawer.begin_frame();
    let second = drawer.batches(r, &figure, &poses, &place);
    for (a, b) in first.iter().zip(&second) {
        assert!(Arc::ptr_eq(&a.vertices, &b.vertices));
        assert_eq!(a.places(), b.places(), "the same list of places");
    }
    let mut moved = poses.clone();
    for pose in &mut moved {
        pose.world = pose.world.then(&Mat4::from_parts(
            Vec3::new(0.0, 0.0, 1.0),
            Quat::IDENTITY,
            Vec3::new(1.0, 1.0, 1.0),
        ));
    }
    drawer.begin_frame();
    let third = drawer.batches(r, &figure, &moved, &place);
    for (a, b) in first.iter().zip(&third) {
        assert!(Arc::ptr_eq(&a.vertices, &b.vertices));
        assert_ne!(a.places(), b.places(), "new places");
    }
    drawer.begin_frame();
    drawer.begin_frame();
    let fourth = drawer.batches(r, &figure, &poses, &place);
    for (a, b) in first.iter().zip(&fourth) {
        assert_ne!(a.places(), b.places(), "made again");
        assert_eq!(a.posed, b.posed, "to the same places");
    }
}
