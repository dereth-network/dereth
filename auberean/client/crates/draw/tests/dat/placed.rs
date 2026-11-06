//! Placed objects that play: one body for each look, its pose worn by every object showing the
//! look, each draw's box about where the pose puts it.

use std::sync::Arc;

use auberean_animation::CachedRecords;
use auberean_draw::body::BodyDrawer;
use auberean_draw::placed::PlacedBodies;
use auberean_draw::scene::{Camera, Cull, Frame, ObjectBatch, SceneBuilder, Spot};
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;
use auberean_world::place::{parts_of, Part};

use crate::common::install;

/// A school of nine fish, each a bone of the one mesh set.
const FISH: DataId = DataId(0x4700_0268);
/// A tree the wind moves.
const TREE: DataId = DataId(0x4700_0002);
/// A look with weighted meshes and no behavior table.
const STILL: DataId = DataId(0x4700_0018);

const STEP: f32 = 1.0 / 30.0;

fn place(x: f32, scale: f32) -> (Mat4, Vec3) {
    (
        Mat4::from_parts(
            Vec3::new(x, 0.0, 0.0),
            Quat::IDENTITY,
            Vec3::new(1.0, 1.0, 1.0),
        ),
        Vec3::new(scale, scale, scale),
    )
}

/// A frame of the parts given, seen from nearby.
fn frame_of(parts: &[Part]) -> Frame {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.45,
        day: None,
    };
    let camera = Camera {
        position: Vec3::new(0.0, -5.0, 2.0),
        heading: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_y: 60.0,
        near: 0.1,
        far: 500.0,
        orthographic: None,
    };
    let day = builder.day(&camp);
    let light = builder.light(&camp, day.as_ref());
    builder.parts_frame(parts, Vec::new(), camera, light, None)
}

/// What steps and poses a frame's objects.
struct Player<'r> {
    records: CachedRecords<'r>,
    bodies: PlacedBodies,
    drawer: BodyDrawer,
}

impl Player<'_> {
    fn new(wind: f32) -> Self {
        let mut records = CachedRecords::new(&install().records);
        records.wind = wind;
        Self {
            records,
            bodies: PlacedBodies::default(),
            drawer: BodyDrawer::default(),
        }
    }

    fn pose(&mut self, frame: &mut Frame) {
        self.drawer.begin_frame();
        self.bodies.pose(
            &install().records,
            &self.records,
            &mut self.drawer,
            &mut frame.objects,
        );
    }

    /// `steps` steps, the frame drawn after each.
    fn pass(&mut self, frame: &mut Frame, steps: u32) {
        for _ in 0..steps {
            self.bodies.advance(&self.records, STEP);
            self.pose(frame);
        }
    }
}

fn world_points(b: &ObjectBatch) -> Vec<Vec3> {
    let mut out = Vec::new();
    b.each_position(|[x, y, z]| out.push(b.transform.point(Vec3::new(x, y, z))));
    out
}

fn apart(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// The farthest any vertex stands between two poses of a frame's first played draw.
fn moved(before: &[Vec3], after: &[Vec3]) -> f32 {
    assert_eq!(before.len(), after.len());
    before
        .iter()
        .zip(after)
        .map(|(a, b)| apart(*a, *b))
        .fold(0.0, f32::max)
}

/// The fish swim: posed, the mesh's vertices are carried metres from where a moment before they
/// stood, though nothing moved the object. Two objects showing the look are in one pose: a mesh's
/// list is the one both draw, each where it stands and at its own scale, so a body's work is done
/// once whatever shows it.
///
/// Behaviour: draw.world.a-placed-look-with-a-behavior-table-plays-through-one-shared-body
#[test]
fn a_placed_school_of_fish_swims_and_every_object_showing_it_moves_together() {
    let r = &install().records;
    let (at_a, scale_a) = place(0.0, 1.0);
    let (at_b, scale_b) = place(40.0, 2.0);
    let mut parts = parts_of(r, FISH, &at_a, scale_a);
    let one_object = parts.len();
    assert!(one_object > 1, "the school's parts: {one_object}");
    parts.extend(parts_of(r, FISH, &at_b, scale_b));
    let mut frame = frame_of(&parts);
    let mut player = Player::new(0.0);

    player.pose(&mut frame);
    let played = |f: &Frame| -> Vec<ObjectBatch> {
        f.objects
            .iter()
            .filter(|b| b.played.is_some())
            .cloned()
            .collect()
    };
    let first = played(&frame);
    assert!(!first.is_empty(), "the school's draws");
    assert!(first.iter().all(|b| {
        b.posed
            .as_ref()
            .is_some_and(|list| list.len() == b.vertices.len())
    }));
    let of = |batches: &[ObjectBatch], x: f32, mesh: u32| -> ObjectBatch {
        batches
            .iter()
            .find(|b| b.mesh == mesh && (b.transform.0[3][0] - x).abs() < 0.01)
            .cloned()
            .expect("the mesh's draw at the object")
    };
    let mesh = first[0].mesh;
    let (a, b) = (of(&first, 0.0, mesh), of(&first, 40.0, mesh));
    assert!(
        Arc::ptr_eq(a.posed.as_ref().unwrap(), b.posed.as_ref().unwrap()),
        "both objects draw the one list"
    );
    for (pa, pb) in world_points(&a).iter().zip(world_points(&b)) {
        assert!(
            (pb.x - 40.0 - 2.0 * pa.x).abs() < 0.001
                && (pb.y - 2.0 * pa.y).abs() < 0.001
                && (pb.z - 2.0 * pa.z).abs() < 0.001,
            "the second object is the first at twice the size, 40 m on: {pa:?} {pb:?}"
        );
    }

    let before = world_points(&a);
    player.pass(&mut frame, 120);
    let after = played(&frame);
    let far = moved(&before, &world_points(&of(&after, 0.0, mesh)));
    println!("the school moved {far:.2} m in 4 s");
    assert!(far > 2.0, "the school swims: {far} m in 4 s");
}

/// Wherever its animation takes it over a whole lap, every vertex of a draw stands in the box the
/// draw is culled by, though the box as stored holds nothing like that reach.
///
/// Behaviour: draw.world.a-placed-objects-box-holds-it-where-its-animation-takes-it
#[test]
fn a_placed_objects_box_holds_it_wherever_its_animation_takes_it() {
    let r = &install().records;
    let (at, scale) = place(10.0, 1.0);
    let parts = parts_of(r, FISH, &at, scale);
    let mut frame = frame_of(&parts);
    let stored: Vec<Cull> = frame.objects.iter().map(|b| b.cull).collect();
    let mut player = Player::new(0.0);
    let mut outside_stored = 0;
    for _ in 0..40 {
        player.pose(&mut frame);
        for (b, stored) in frame.objects.iter().zip(&stored) {
            let Cull::Box { lo, hi } = b.cull else {
                panic!("a placed object culled by a box: {:?}", b.cull);
            };
            let Cull::Box { lo: l0, hi: h0 } = *stored else {
                panic!("a placed object culled by a box");
            };
            for p in world_points(b) {
                let e = 0.001;
                assert!(
                    p.x >= lo.x - e
                        && p.y >= lo.y - e
                        && p.z >= lo.z - e
                        && p.x <= hi.x + e
                        && p.y <= hi.y + e
                        && p.z <= hi.z + e,
                    "{p:?} outside {lo:?} {hi:?}"
                );
                outside_stored += u32::from(p.x < l0.x || p.y < l0.y || p.x > h0.x || p.y > h0.y);
            }
        }
        player.pass(&mut frame, 15);
    }
    assert!(outside_stored > 0, "the school leaves its box as stored");
}

/// The wind moves a tree that plays: its canopy travels further in a gale than in still air.
///
/// Behaviour: draw.world.the-wind-moves-a-placed-object-that-plays
#[test]
fn the_wind_moves_a_placed_tree_further_than_still_air_does() {
    let r = &install().records;
    let (at, scale) = place(0.0, 1.0);
    let parts = parts_of(r, TREE, &at, scale);
    let sway = |wind: f32| {
        let mut frame = frame_of(&parts);
        let mut player = Player::new(wind);
        player.pose(&mut frame);
        let start: Vec<Vec<Vec3>> = frame.objects.iter().map(world_points).collect();
        let mut most = 0.0f32;
        for _ in 0..30 {
            player.pass(&mut frame, 6);
            for (b, s) in frame.objects.iter().zip(&start) {
                most = most.max(moved(s, &world_points(b)));
            }
        }
        most
    };
    let (calm, gale) = (sway(0.0), sway(1.0));
    println!("the tree moves {calm:.3} m in still air, {gale:.3} m in a gale");
    assert!(calm > 0.0, "the tree plays in still air");
    assert!(gale > 2.0 * calm, "{gale} in a gale, {calm} in still air");
}

/// A look with no behavior table has nothing to play: it stands as stored.
///
/// Behaviour: draw.world.a-placed-look-with-a-behavior-table-plays-through-one-shared-body
#[test]
fn a_placed_look_with_no_behavior_table_stands_as_stored() {
    let r = &install().records;
    let (at, scale) = place(0.0, 1.0);
    let parts = parts_of(r, STILL, &at, scale);
    assert!(!parts.is_empty());
    assert!(parts.iter().all(|p| p.played.is_none()));
    let mut frame = frame_of(&parts);
    let mut player = Player::new(0.0);
    player.pose(&mut frame);
    assert!(frame.objects.iter().all(|b| b.posed.is_none()));
}
