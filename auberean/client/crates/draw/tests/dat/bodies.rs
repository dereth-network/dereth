//! What a body's draws keep from frame to frame: the mesh's own list always, and the list of places
//! and facings for as long as the pose stands.
//!
//! Behaviour: none (how the lists are kept, which no behavior of the game names)

use std::sync::Arc;

use auberean_animation::body::Body as Figure;
use auberean_animation::skin::{influences, skin};
use auberean_animation::CachedRecords;
use auberean_assets::mesh::Mesh;
use auberean_draw::body::{answer_ahead, ask_ahead, BodyDrawer};
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

/// The player's own meshes keep their detail farther from the camera than another's: at some
/// distance the same figure is drawn with more triangles as the player, and at none with fewer.
///
/// Behaviour: draw.bodies.the-players-meshes-keep-their-detail-farther-out
#[test]
fn the_players_meshes_keep_their_detail_farther_out() {
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let (figure, place) = figure();
    let poses = figure.pose(&recs);
    let triangles = |player: bool, distance: f32| {
        let mut drawer = BodyDrawer::default();
        drawer.eye = Some(Vec3::new(10.0 + distance, 20.0, 30.0));
        drawer.drawing_player = player;
        drawer
            .batches(r, &figure, &poses, &place)
            .iter()
            .map(|b| b.indices.len())
            .sum::<usize>()
    };
    let mut kept = 0;
    for d in 1..60_u16 {
        let distance = f32::from(d);
        let (mine, another) = (triangles(true, distance), triangles(false, distance));
        assert!(mine >= another, "at {distance} m: {mine} against {another}");
        kept += usize::from(mine > another);
    }
    assert!(kept > 0, "it keeps more at some distance");
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

/// A body's lists and boxes worked out ahead on the threads there are, are to the bit what they are
/// when each is worked out as it is asked for.
#[test]
fn a_bodys_lists_are_the_same_worked_out_on_threads() {
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let (figure, place) = figure();
    let poses = figure.pose(&recs);
    let spin = (0.28, 0.96);
    let ahead = BodyDrawer::default().batches(r, &figure, &poses, &place);
    let asked = BodyDrawer::default().batches(r, &figure, &poses, &place);
    let mut asks = Vec::new();
    let weight = ask_ahead(&ahead, spin, true, &mut asks);
    assert!(weight > 1000, "{weight} vertices to walk");
    assert!(asks.len() > 3, "{} lists to work out", asks.len());
    answer_ahead(asks);
    let mut again = Vec::new();
    assert_eq!(ask_ahead(&ahead, spin, true, &mut again), 0, "all kept");
    for (a, b) in ahead.iter().zip(&asked) {
        let (x, y) = (
            a.posed.as_ref().expect("posed"),
            b.posed.as_ref().expect("posed"),
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&x[..]),
            bytemuck::cast_slice::<_, u8>(&y[..])
        );
        assert_eq!(a.turned_box(spin), b.turned_box(spin));
        assert_eq!(a.turned_box((0.0, 1.0)), b.turned_box((0.0, 1.0)));
        assert_eq!(x.positions(), y.positions());
    }
}
