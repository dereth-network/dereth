//! A body's detections: the spheres it keeps around objects and around itself, the changes the
//! cells report to it, and the detections it makes at once; and the world's side of them: the
//! bodies watching each cell and what they are told, the lines of sight the cells trace, the
//! detections made against what stands in the world, the rays that cover an object, and a ray cut
//! at landblock edges.
//! Fixture: a made-up world of objects at places, cells counted by who watches them, and lines of
//! sight that stop where the test says; and the world itself over the flat 160 m block at height
//! 10 of the objects tests, with their 0.48 m bodies and upright posts, a made-up interior cell
//! holding one wall across it, and made-up game objects that answer for the ids a test names.

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_assets::Context;
use auberean_dat::Reader;
use auberean_physics::cells::{block_list, block_ray_clip, direction};
use auberean_physics::detect::manager::{outcome, status, DetectionHost, Detections};
use auberean_physics::detect::{
    BlockRays, Detected, Detector, DetectorSettings, ReportType, Sighting, Sightings,
};
use auberean_physics::ethereal::{PhysicsTypes, SightLine};
use auberean_physics::geom::cover::{box_cover_rays, sphere_cover_rays};
use auberean_physics::geom::{BBox, Ray};
use auberean_physics::land::Land;
use auberean_physics::object::Shape;
use auberean_physics::pack::{Pack, Writer};
use auberean_physics::world::{self as table, split_ray, GameObjects, WorldObject};
use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Polygon, Sphere};
use dereth_physics::source::EnvCellGeometry;
use dereth_primitives::shape::{CylSphere, Plane};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::contact::objects::{at as standing, body, land, post};

const BODY: u64 = 0x50;
const CELL: u32 = 0x4040_0001;

fn at(x: f32, y: f32) -> Position {
    Position::new(
        CellId(CELL),
        Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
    )
}

/// A world of objects (place and radius), the watchers of each cell, a clock, and a line of sight
/// that hits something after `los_reach` meters (none: it runs its whole length clear).
#[derive(Default)]
struct World {
    body: Option<Position>,
    objects: BTreeMap<u64, (Position, f32)>,
    watched: BTreeMap<u32, BTreeMap<u64, u32>>,
    now: f64,
    los_reach: Option<f32>,
    reports: Vec<(u32, Detected)>,
}

impl DetectionHost for World {
    fn id(&self) -> u64 {
        BODY
    }
    fn position(&self) -> Position {
        self.body.unwrap_or_else(|| at(10.0, 10.0))
    }
    fn object(&self, id: u64) -> Option<(Position, f32)> {
        self.objects.get(&id).copied()
    }
    fn offset_position(&self, id: u64, _h: f32, _s: bool, offset: Vec3) -> Option<Position> {
        let mut p = if id == 0 {
            self.position()
        } else {
            self.objects.get(&id)?.0
        };
        p.frame.origin = Vec3::new(
            p.frame.origin.x + offset.x,
            p.frame.origin.y + offset.y,
            p.frame.origin.z + offset.z,
        );
        Some(p)
    }
    fn top_level_position(&self, id: u64) -> Option<Position> {
        self.objects.get(&id).map(|o| o.0)
    }
    fn add_targeted(&mut self, id: u64, _s: &DetectorSettings, sightings: &mut Sightings) {
        sightings.entry(id).or_default().object = id;
    }
    fn cells_within(&self, at: &Position, radius: f32) -> Vec<(CellId, bool)> {
        // One 10 m cell per ten metres of x, all loaded.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let lo = ((at.frame.origin.x - radius) / 10.0).floor().max(0.0) as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hi = ((at.frame.origin.x + radius) / 10.0).floor().max(0.0) as u32;
        (lo..=hi).map(|i| (CellId(CELL + i), true)).collect()
    }
    fn watch_cell(&mut self, cell: CellId, watcher: u64, _d: &Detector) {
        *self
            .watched
            .entry(cell.0)
            .or_default()
            .entry(watcher)
            .or_default() += 1;
    }
    fn unwatch_cell(&mut self, cell: CellId, watcher: u64, _d: &Detector) {
        if let Some(w) = self.watched.get_mut(&cell.0) {
            if let Some(n) = w.get_mut(&watcher) {
                *n -= 1;
                if *n == 0 {
                    w.remove(&watcher);
                }
            }
        }
    }
    fn watchers(&self) -> Vec<u64> {
        Vec::new()
    }
    fn line_of_sight(&mut self, los: &mut SightLine) {
        let ray = los.initial_ray;
        los.final_cell = los.initial_cell;
        match self.los_reach {
            Some(d) if d < ray.length => {
                los.trace_result = 1;
                los.final_ray = Ray::from_offset(ray.point, ray.dir.mul(d));
            }
            _ => {
                los.trace_result = 0;
                los.final_ray = ray;
            }
        }
    }
    fn block_loaded(&self, _block: u32) -> bool {
        true
    }
    fn blocks_of(&self, cell: CellId, _area: &BBox) -> Vec<u32> {
        vec![cell.0 & 0xFFFF_0000 | 0xFFFF]
    }
    fn intersect_block(
        &mut self,
        _block: u32,
        _s: &DetectorSettings,
        _area: &BBox,
        sightings: &mut Sightings,
        _rays: Option<&mut BlockRays>,
    ) {
        for id in self.objects.keys() {
            sightings.entry(*id).or_default().object = *id;
        }
    }
    fn detect_block(&mut self, _s: &DetectorSettings, _rays: &mut BlockRays) {}
    fn split_ray(&self, ray: &Ray, from: u32) -> Vec<(u32, Ray)> {
        vec![(from, *ray)]
    }
    fn report(&mut self, context: u32, detected: &Detected) {
        self.reports.push((context, *detected));
    }
    fn now(&self) -> f64 {
        self.now
    }
}

trait Mul {
    fn mul(self, k: f32) -> Self;
}

impl Mul for Vec3 {
    fn mul(self, k: f32) -> Self {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
}

/// A sphere around an object notices it entering once the two are nearer than the sphere's radius
/// and the object's together (exactly that far is outside), and leaving once they are not, or
/// once the object is gone from the world; each change is reported once.
///
/// Behaviour: collision.detect.a-sphere-notices-its-object-enter-and-leave
#[test]
fn a_sphere_notices_its_object_enter_and_leave() {
    let mut w = World::default();
    w.objects.insert(0x60, (at(16.0, 10.0), 1.0));
    let mut d = Detections::new();
    assert!(d.create_detector(7, 0x60, 5.0, ReportType::default(), &mut w));
    d.check_detection(true, &mut w);
    assert!(w.reports.is_empty(), "6 m away is not inside 5 + 1");
    w.objects.insert(0x60, (at(15.9, 10.0), 1.0));
    d.check_detection(true, &mut w);
    d.check_detection(true, &mut w);
    assert_eq!(w.reports.len(), 1);
    assert_eq!(w.reports[0].1.status, status::ENTERED);
    w.objects.remove(&0x60);
    d.check_detection(true, &mut w);
    assert_eq!(w.reports.len(), 2);
    assert_eq!(w.reports[1].1.status, status::LEFT);
}

/// The spheres are checked a second after the last check, or when forced; a sphere marked for
/// destruction stops counting at once and is destroyed at the next check.
///
/// Behaviour: collision.detect.spheres-are-checked-each-second-and-destroyed-at-a-check
#[test]
fn spheres_are_checked_each_second_and_destroyed_at_a_check() {
    let mut w = World::default();
    w.objects.insert(0x60, (at(12.0, 10.0), 1.0));
    let mut d = Detections::new();
    d.create_detector(7, 0x60, 5.0, ReportType::default(), &mut w);
    w.now = 0.5;
    d.check_detection(false, &mut w);
    assert!(w.reports.is_empty(), "not yet a second");
    w.now = 1.0;
    d.check_detection(false, &mut w);
    assert_eq!(w.reports.len(), 1);
    assert!(d.is_active());
    assert!(d.destroy_detector(7));
    assert!(!d.has_detector(7));
    assert_eq!(d.num_targeted(), 0);
    assert!(!d.is_active());
    assert!(d.detectors.contains_key(&7), "kept until the next check");
    d.check_detection(false, &mut w);
    assert!(d.detectors.is_empty());
    assert!(!d.destroy_detector(7));
}

/// What the cells report to the global detection: an object entering becomes known; one known
/// that leaves before the leaving is reported, then enters again, is as it was; one that enters
/// and leaves before the entry is reported is forgotten; each report forgets those that left.
///
/// Behaviour: collision.detect.the-global-detection-takes-the-cells-changes
#[test]
fn the_global_detection_takes_the_cells_changes() {
    let mut w = World::default();
    let mut d = Detections::new();
    assert!(d.create_detector(1, 0, 15.0, ReportType::default(), &mut w));
    assert!(
        !d.create_detector(2, 0, 5.0, ReportType::default(), &mut w),
        "one global only"
    );
    let entered = |object| Detected {
        object,
        status: status::ENTERED,
    };
    let left = |object| Detected {
        object,
        status: status::LEFT,
    };
    d.receive_update(&entered(0x60));
    d.receive_update(&entered(0x61));
    assert_eq!(d.pending_updates, 2);
    d.check_detection(true, &mut w);
    assert_eq!(w.reports.len(), 2);
    assert_eq!(d.pending_updates, 0);
    d.receive_update(&left(0x60));
    d.receive_update(&entered(0x60));
    assert_eq!(d.pending_updates, 0, "the leaving is cancelled");
    d.receive_update(&entered(0x62));
    d.receive_update(&left(0x62));
    assert!(!d.known.as_ref().expect("known").contains_key(&0x62));
    d.receive_update(&left(0x61));
    d.check_detection(true, &mut w);
    assert_eq!(w.reports.len(), 3, "only the leaving one is reported");
    let known = d.known.as_ref().expect("known");
    assert!(known.contains_key(&0x60) && !known.contains_key(&0x61));
}

/// The global detection watches the cells within its radius of the body, and works them out again
/// only once the body has moved 5 m: a cell it still reaches stays watched once; destroying it stops
/// every watch.
///
/// Behaviour: collision.detect.the-global-detection-watches-the-cells-around-the-body
#[test]
fn the_global_detection_watches_the_cells_around_the_body() {
    let mut w = World::default();
    let mut d = Detections::new();
    d.create_detector(1, 0, 15.0, ReportType::default(), &mut w);
    let watched = |w: &World| -> Vec<u32> {
        w.watched
            .iter()
            .filter(|(_, by)| by.get(&BODY).is_some_and(|n| *n == 1))
            .map(|(c, _)| c - CELL)
            .collect()
    };
    assert_eq!(watched(&w), vec![0, 1, 2]);
    w.body = Some(at(14.9, 10.0));
    d.check_detection(true, &mut w);
    assert_eq!(watched(&w), vec![0, 1, 2], "under 5 m: unchanged");
    w.body = Some(at(15.0, 10.0));
    d.check_detection(true, &mut w);
    assert_eq!(watched(&w), vec![0, 1, 2, 3]);
    assert!(d.destroy_detector(1));
    d.check_detection(true, &mut w);
    assert!(watched(&w).is_empty());
    assert!(d.known.is_none() && d.cells.is_none());
}

/// A detection made at once stands on the body when it has no offset; with a target it is placed
/// at the target as far as a line of sight gets, and blocked when the line hits something and it
/// is not to be made there; a target not in the world, or beyond the detection's reach, leaves it
/// not placed.
///
/// Behaviour: collision.detect.a-detection-made-at-once-finds-its-origin
#[test]
fn a_detection_made_at_once_finds_its_origin() {
    let mut w = World::default();
    w.objects.insert(0x60, (at(20.0, 10.0), 1.0));
    let mut d = Detections::new();
    let mut s = DetectorSettings::default();
    s.set_shape_sphere(3.0, Vec3::ZERO);
    assert!(d.init_intersection(4, &s));
    assert!(!d.init_intersection(4, &s), "already running");
    let mut out = 99;
    assert!(d.do_intersection(4, &mut out, &mut w));
    assert_eq!(out, outcome::SUCCEEDED);
    assert_eq!(
        d.state(4).settings.origin_point().frame.origin,
        Vec3::new(10.0, 10.0, 0.0)
    );
    assert!(
        !d.state(4).settings.based_on_object,
        "the base is kept as a place"
    );

    let target = |max: f32, at_blocked: bool| {
        let mut s = DetectorSettings::default();
        s.set_shape_sphere(3.0, Vec3::ZERO);
        s.center_on_target = 0x60;
        s.max_target_distance = max;
        s.detect_at_blocked_offset = at_blocked;
        s
    };
    let mut d = Detections::new();
    d.init_intersection(5, &target(50.0, false));
    assert!(d.do_intersection(5, &mut out, &mut w));
    assert_eq!(out, outcome::SUCCEEDED);
    assert_eq!(
        d.state(5).settings.origin_point().frame.origin,
        Vec3::new(20.0, 10.0, 0.0)
    );

    w.los_reach = Some(4.0);
    let mut d = Detections::new();
    d.init_intersection(5, &target(50.0, false));
    d.do_intersection(5, &mut out, &mut w);
    assert_eq!(out, outcome::BLOCKED);
    let mut d = Detections::new();
    d.init_intersection(5, &target(50.0, true));
    d.do_intersection(5, &mut out, &mut w);
    assert_eq!(out, outcome::SUCCEEDED);
    assert_eq!(
        d.state(5).settings.origin_point().frame.origin,
        Vec3::new(14.0, 10.0, 0.0)
    );

    let mut d = Detections::new();
    d.init_intersection(5, &target(5.0, false));
    d.do_intersection(5, &mut out, &mut w);
    assert_eq!(out, outcome::NOT_PLACED, "beyond reach");
    w.objects.clear();
    let mut d = Detections::new();
    d.init_intersection(5, &target(50.0, false));
    d.do_intersection(5, &mut out, &mut w);
    assert_eq!(out, outcome::NOT_PLACED, "no target");
}

/// A body's detections keep their spheres by context, the time of the last check, and the
/// detections made at once, through their binary form.
///
/// Behaviour: collision.detect.a-bodys-detections-survive-packing
#[test]
fn a_bodys_detections_survive_packing() {
    let mut w = World::default();
    let mut d = Detections::new();
    d.create_detector(7, 0x60, 5.0, ReportType { mask: 3 }, &mut w);
    d.last_check = 12.5;
    let mut s = DetectorSettings::default();
    s.set_shape_sphere(3.0, Vec3::ZERO);
    d.init_intersection(4, &s);
    let mut wr = Writer::new();
    d.pack(&mut wr);
    let b = wr.finish();
    let back = Detections::unpack(&mut Reader::new(&b), &Context::default()).expect("reads");
    assert_eq!(back.detectors, d.detectors);
    assert_eq!(back.last_check, 12.5);
    assert_eq!(back.instant.keys().collect::<Vec<_>>(), vec![&4]);
}

fn a_body() -> auberean_physics::body::Body {
    use auberean_physics::body::{Body, BodyConfig};
    use dereth_physics::geom::Sphere;
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        step_up: 0.5,
        step_down: 0.5,
        scale: 2.0,
        ground: Default::default(),
        air: Default::default(),
    };
    let mut b = Body::new(config, at(10.0, 10.0), 0.0);
    b.setup_radius = 0.4;
    b.setup_height = 1.8;
    b
}

/// A body's detection sphere around an object reaches the body's own radius (its setup's, by its
/// look's scale and its own) further than asked; the body counts the spheres it keeps, by object
/// and around itself, and ends them when asked.
///
/// Behaviour: collision.detect.a-bodys-sphere-reaches-its-own-radius-further
#[test]
fn a_bodys_sphere_reaches_its_own_radius_further() {
    let mut w = World::default();
    w.objects.insert(0x60, (at(16.0, 10.0), 1.0));
    let mut b = a_body();
    assert!((b.radius() - 0.8).abs() < 1e-6 && (b.height() - 3.6).abs() < 1e-6);
    b.set_scale(1.5);
    assert!(
        (b.radius() - 1.2).abs() < 1e-6 && (b.height() - 5.4).abs() < 1e-6,
        "its own scale too: {} {}",
        b.radius(),
        b.height()
    );
    b.set_scale(1.0);
    b.set_misc_active(false);
    assert!(b.create_detection_cylsphere(7, 0x60, 5.0, ReportType::default(), &mut w));
    assert!(b.is_misc_active(), "its other updates wake to check it");
    let d = b.detections.as_ref().expect("made").detectors[&7];
    assert!((d.radius - 5.8).abs() < 1e-6, "{}", d.radius);
    assert!(b.has_detection_cylsphere(7));
    assert_eq!(b.num_targeted_detection_cylspheres_on(0x60), 1);
    assert_eq!(b.num_targeted_detection_cylspheres_on(0), 0);
    assert_eq!(b.num_targeted_detection_cylspheres(), 1);
    assert_eq!(b.num_global_detection_cylspheres(), 0);
    assert!(b.delete_detection_cylsphere(7));
    assert_eq!(b.num_targeted_detection_cylspheres(), 0);
}

/// A place on a body straight above an outdoor place is just raised part of the body's height; a
/// place off to one side is reached by a line of sight through the cells' walls only, from the
/// body's origin, and stops where that line stops.
///
/// Behaviour: collision.detect.a-place-on-a-body-is-raised-or-traced-through-walls
#[test]
fn a_place_on_a_body_is_raised_or_traced_through_walls() {
    use auberean_physics::body::ENVIRONMENT_ONLY;
    let b = a_body();
    let mut asked = 0;
    let (up, clear) = b.object_offset_position(0.5, false, Vec3::ZERO, &mut |_| asked += 1);
    assert!(clear && asked == 0);
    assert!(
        (up.frame.origin.z - 1.8).abs() < 1e-6,
        "{:?}",
        up.frame.origin
    );

    let mut w = World {
        los_reach: Some(1.0),
        ..World::default()
    };
    let mut kind = None;
    let (side, clear) = b.object_offset_position(0.0, true, Vec3::new(2.0, 0.0, 0.0), &mut |l| {
        kind = Some(l.intersect);
        w.line_of_sight(l);
    });
    assert_eq!(kind, Some(ENVIRONMENT_ONLY));
    assert!(!clear, "the line stopped");
    assert!(
        (side.frame.origin.x - 11.0).abs() < 1e-5,
        "{:?}",
        side.frame.origin
    );
}

const WATCHER: u64 = 0x10;
const MOVER: u64 = 0x20;
const POST: u64 = 0x30;

/// Game objects after the ids they hold.
struct After(Vec<u64>);

impl GameObjects for After {
    fn matches(&self, object: u64, _report: &ReportType) -> bool {
        self.0.contains(&object)
    }
    fn note(&self, _object: u64, _sighting: &mut Sighting) {}
}

fn put(
    world: &mut table::World,
    land: &Land,
    id: u64,
    shape: Arc<Shape>,
    p: Position,
    still: bool,
) {
    world.insert(
        land,
        id,
        &WorldObject {
            shape,
            position: p,
            is_static: still,
            passable: false,
        },
    );
}

/// A post `height` meters high.
fn tall_post(height: f32) -> Arc<Shape> {
    Arc::new(Shape {
        cylinders: vec![CylSphere {
            low_pt: Vec3::ZERO,
            radius: 0.5,
            height,
        }],
        ..(*post()).clone()
    })
}

fn entered(object: u64) -> (u64, Detected) {
    (
        WATCHER,
        Detected {
            object,
            status: status::ENTERED,
        },
    )
}

fn left(object: u64) -> (u64, Detected) {
    (
        WATCHER,
        Detected {
            object,
            status: status::LEFT,
        },
    )
}

/// A body watching a cell is told when another body comes to stand in it and when it leaves (by
/// moving into another cell or out of the world), and nothing while it moves about within the
/// cell; it is never told of itself, of an object that never moves, or of an undetectable one, and
/// hiding a body in the cell tells it the body left, showing it again that it entered.
///
/// Behaviour: detection.watch.a-watcher-is-told-what-comes-and-goes
#[test]
fn a_watcher_is_told_what_comes_and_goes() {
    let land = land();
    let mut world = table::World::new();
    let mut w = body(50.0, 50.0);
    let wk = world.insert_body(&land, WATCHER, &mut w);
    let cell = world.own_cell(wk).expect("standing in a cell");
    assert!(world.add_watcher(cell, WATCHER, ReportType::default()));
    world.place(&land, wk, standing(75.0, 50.0));
    world.place(&land, wk, standing(50.0, 50.0));
    assert!(world.take_detection_updates().is_empty(), "not of itself");

    put(&mut world, &land, POST, post(), standing(52.0, 52.0), true);
    assert!(
        world.take_detection_updates().is_empty(),
        "not of a still object"
    );

    let mut m = body(75.0, 50.0);
    let mk = world.insert_body(&land, MOVER, &mut m);
    assert!(world.take_detection_updates().is_empty(), "another cell");
    world.place(&land, mk, standing(55.0, 50.0));
    assert_eq!(world.take_detection_updates(), vec![entered(MOVER)]);
    world.place(&land, mk, standing(56.0, 51.0));
    assert!(world.take_detection_updates().is_empty(), "within the cell");
    world.place(&land, mk, standing(75.0, 50.0));
    assert_eq!(world.take_detection_updates(), vec![left(MOVER)]);

    world.place(&land, mk, standing(55.0, 50.0));
    world.set_undetectable(mk, true);
    world.set_undetectable(mk, false);
    assert_eq!(
        world.take_detection_updates(),
        vec![entered(MOVER), left(MOVER), entered(MOVER)]
    );
    world.set_undetectable(mk, true);
    world.take_detection_updates();
    world.place(&land, mk, standing(75.0, 50.0));
    world.place(&land, mk, standing(55.0, 50.0));
    assert!(
        world.take_detection_updates().is_empty(),
        "not undetectable"
    );
    world.set_undetectable(mk, false);
    world.take_detection_updates();
    assert!(world.remove(mk));
    assert_eq!(world.take_detection_updates(), vec![left(MOVER)]);
}

/// A body's own setters reach the world it moves in: made undetectable, its cell's watchers hear
/// it leave the next time it ticks (once: hidden already, it is not told of again); made
/// detectable, they hear it come back, and again for each time it is so made (as the client tells
/// them each time); ticking with nothing asked tells them nothing. Its state word set whole does
/// the same when its detectable bit changes.
///
/// Behaviour: detection.watch.a-bodys-own-hiding-reaches-its-cells-watchers
#[test]
fn a_bodys_own_hiding_reaches_its_cells_watchers() {
    let land = land();
    let mut world = table::World::new();
    let mut w = body(50.0, 50.0);
    let wk = world.insert_body(&land, WATCHER, &mut w);
    let cell = world.own_cell(wk).expect("standing in a cell");
    assert!(world.add_watcher(cell, WATCHER, ReportType::default()));
    let mut m = body(55.0, 50.0);
    world.insert_body(&land, MOVER, &mut m);
    world.take_detection_updates();
    m.set_undetectable(true);
    m.set_undetectable(true);
    m.tick_in(&land, &mut world, 0.1);
    assert_eq!(world.take_detection_updates(), vec![left(MOVER)]);
    m.tick_in(&land, &mut world, 0.1);
    assert!(world.take_detection_updates().is_empty(), "nothing asked");
    m.set_state(m.state() & !0x800);
    m.set_undetectable(false);
    m.tick_in(&land, &mut world, 0.1);
    assert_eq!(
        world.take_detection_updates(),
        vec![entered(MOVER), entered(MOVER)]
    );
    assert!(!m.is_undetectable());
}

/// A body's second watch of a cell only counts up and tells it nothing; the first tells it of
/// each object already standing there, as entering, and only the last to end tells it of each, as
/// leaving. Through a body's own global detection, the cells around it are watched and what it is
/// told reaches what the detection knows.
///
/// Behaviour: detection.watch.watches-are-counted-and-the-first-and-last-tell
#[test]
fn watches_are_counted_and_the_first_and_last_tell() {
    let land = land();
    let mut world = table::World::new();
    let mut w = body(110.0, 110.0);
    let wk = world.insert_body(&land, WATCHER, &mut w);
    let mut m = body(55.0, 50.0);
    let mk = world.insert_body(&land, MOVER, &mut m);
    let cell = world.own_cell(mk).expect("standing in a cell");
    world.take_detection_updates();

    assert!(world.add_watcher(cell, WATCHER, ReportType::default()));
    assert_eq!(world.take_detection_updates(), vec![entered(MOVER)]);
    assert!(world.add_watcher(cell, WATCHER, ReportType::default()));
    assert_eq!(world.watch_count(cell, WATCHER), 2);
    assert!(world.take_detection_updates().is_empty(), "counted only");
    assert!(world.remove_watcher(cell, WATCHER));
    assert!(
        world.take_detection_updates().is_empty(),
        "one still watches"
    );
    assert!(world.remove_watcher(cell, WATCHER));
    assert_eq!(world.take_detection_updates(), vec![left(MOVER)]);
    assert_eq!(world.watch_count(cell, WATCHER), 0);
    assert!(!world.remove_watcher(cell, WATCHER), "not watching");

    // The global detection of a body standing beside the mover: the cells within 10 m of it.
    world.place(&land, wk, standing(50.0, 50.0));
    w.position = standing(50.0, 50.0);
    let game = After(Vec::new());
    assert!(w.create_detection_cylsphere(
        1,
        0,
        10.0,
        ReportType::default(),
        &mut world.detection(&land, wk, &game),
    ));
    let told = world.take_detection_updates();
    assert_eq!(told, vec![entered(MOVER)]);
    for (_, d) in &told {
        w.receive_detection_update(d);
    }
    let d = w.detections.as_ref().expect("detections");
    assert_eq!(
        d.known
            .as_ref()
            .map(|k| k.keys().copied().collect::<Vec<_>>()),
        Some(vec![MOVER])
    );
    assert_eq!(d.pending_updates, 1);
    assert!(world.watch_count(cell, WATCHER) >= 1);
}

/// A cell with one wall: the plane `y = 5` from -5 to 5 across and up, facing `-y`, solid behind.
fn walled_cell(land: &Land) -> CellId {
    let id = CellId(0x4040_0100);
    let wall = Polygon::new(vec![
        Vec3::new(-5.0, 5.0, -5.0),
        Vec3::new(5.0, 5.0, -5.0),
        Vec3::new(5.0, 5.0, 5.0),
        Vec3::new(-5.0, 5.0, 5.0),
    ]);
    let leaf = |index: i32, solid: bool, polys: Vec<u32>| BspNode {
        sphere: Sphere::new(Vec3::ZERO, 20.0),
        splitting_plane: Plane::default(),
        pos_child: None,
        neg_child: None,
        kind: BspNodeKind::Leaf {
            leaf_index: index,
            solid,
        },
        in_polys: polys,
    };
    let tree = BspTree {
        nodes: vec![
            BspNode {
                sphere: Sphere::new(Vec3::ZERO, 20.0),
                splitting_plane: wall.plane,
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: Vec::new(),
            },
            leaf(0, false, vec![0]),
            leaf(1, true, Vec::new()),
        ],
        polygons: vec![wall.clone()],
    };
    land.insert_cell(EnvCellGeometry {
        id,
        frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        portals: Vec::new(),
        physics_polygons: vec![wall],
        physics_bsp: Some(Arc::new(tree)),
        cell_bsp: None,
        stab_list: Vec::new(),
        seen_outside: false,
        holds_without_portals: true,
        static_objects: Vec::new(),
    });
    id
}

/// A line of sight from `from` to `to` in `cell`, tracing physics shapes.
fn sight(cell: CellId, from: Vec3, to: Vec3) -> SightLine {
    let mut los = SightLine::default();
    los.set_ray(Ray::segment(from, to), cell.0);
    los.set_intersect_type(0);
    los
}

/// A line of sight is stopped by a post standing across it, ending at the post's near side, unless
/// the line passes through the post by its id or traces only the surroundings; inside, a wall
/// stops it where it stands, and a line short of the wall runs its whole length.
///
/// Behaviour: detection.sight.objects-and-walls-stop-a-line-of-sight
#[test]
fn objects_and_walls_stop_a_line_of_sight() {
    let land = land();
    let mut world = table::World::new();
    put(
        &mut world,
        &land,
        POST,
        tall_post(5.0),
        standing(50.0, 55.0),
        true,
    );
    let cell = standing(50.0, 50.0).cell;
    let (from, to) = (Vec3::new(50.0, 50.0, 11.0), Vec3::new(50.0, 60.0, 11.0));

    let mut los = sight(cell, from, to);
    assert!(world.los_check(&land, &mut los));
    assert_eq!(los.trace_result, 1, "stopped");
    let end = los.end_point();
    assert!((end.y - 54.5).abs() < 0.01, "at the post: {end:?}");

    let mut los = sight(cell, from, to);
    los.objects.add_to_ethereal_list(POST);
    assert!(world.los_check(&land, &mut los));
    assert_eq!(los.trace_result, 0, "passed through by id");
    assert!((los.end_point().y - 60.0).abs() < 1e-3);

    let mut los = sight(cell, from, to);
    los.set_intersect_type(auberean_physics::body::ENVIRONMENT_ONLY);
    assert!(world.los_check(&land, &mut los));
    assert_eq!(los.trace_result, 0, "the surroundings only");

    let inside = walled_cell(&land);
    let mut los = sight(inside, Vec3::ZERO, Vec3::new(0.0, 10.0, 0.0));
    assert!(world.los_check(&land, &mut los));
    assert_eq!(los.trace_result, 1, "the wall");
    assert!((los.end_point().y - 5.0).abs() < 1e-3);
    let mut los = sight(inside, Vec3::ZERO, Vec3::new(0.0, 4.0, 0.0));
    assert!(world.los_check(&land, &mut los));
    assert_eq!(los.trace_result, 0, "short of the wall");

    let mut los = sight(CellId(0x5050_0001), from, to);
    assert!(!world.los_check(&land, &mut los), "no loaded cell");
}

/// A detection a body makes at once against the world: a 5 m sphere on the body finds the object
/// it is after standing inside it, not one outside it, not one it is not after; with lines of sight
/// wanted, a target whose one ray a post stands across is hidden and dropped, and with the post
/// gone it is seen.
///
/// Behaviour: detection.intersect.a-detection-made-at-once-finds-what-is-in-its-volume
#[test]
fn a_detection_made_at_once_finds_what_is_in_its_volume() {
    let land = land();
    let mut world = table::World::new();
    let mut b = body(50.0, 50.0);
    let key = world.insert_body(&land, WATCHER, &mut b);
    put(&mut world, &land, POST, post(), standing(53.0, 50.0), true);
    put(
        &mut world,
        &land,
        POST + 1,
        post(),
        standing(57.0, 52.0),
        true,
    );
    put(
        &mut world,
        &land,
        POST + 2,
        post(),
        standing(52.0, 47.0),
        true,
    );
    let game = After(vec![POST, POST + 1, WATCHER]);
    let mut s = DetectorSettings {
        context: 7,
        physics_blocking: PhysicsTypes::new(u64::MAX),
        ..DetectorSettings::default()
    };
    s.set_shape_sphere(5.0, Vec3::ZERO);
    s.add_exclude_object(WATCHER);
    let (found, out) = b
        .do_intersection(&s, &mut world.detection(&land, key, &game))
        .expect("made at once");
    assert_eq!(out, outcome::SUCCEEDED);
    assert_eq!(found.keys().copied().collect::<Vec<_>>(), vec![POST]);
    let seen = &found[&POST];
    assert!((seen.distance - 3.0).abs() < 0.1, "{}", seen.distance);

    // Lines of sight: a body six meters east, behind a post three meters east.
    let mut world = table::World::new();
    let key = world.insert_body(&land, WATCHER, &mut b);
    let mut t = body(56.0, 50.0);
    world.insert_body(&land, MOVER, &mut t);
    put(
        &mut world,
        &land,
        POST,
        tall_post(5.0),
        standing(53.0, 50.0),
        true,
    );
    let game = After(vec![MOVER]);
    let mut s = DetectorSettings {
        context: 8,
        ..DetectorSettings::default()
    };
    s.set_shape_sphere(8.0, Vec3::ZERO);
    s.object_blocking.add_to_ethereal_list(WATCHER);
    let (found, _) = b
        .do_intersection(&s, &mut world.detection(&land, key, &game))
        .expect("made at once");
    assert!(found.is_empty(), "hidden: {found:?}");
    let post_key = world.find(POST).expect("the post");
    world.remove(post_key);
    let (found, _) = b
        .do_intersection(&s, &mut world.detection(&land, key, &game))
        .expect("made at once");
    assert_eq!(found.keys().copied().collect::<Vec<_>>(), vec![MOVER]);
    assert_eq!(found[&MOVER].los_coverage, 0.0);
}

/// Every search and every move counts the world's one stamp on, and an object is taken once a
/// count: a detection centered on an object takes it again in the same tick, a second taking with
/// nothing between does not, and one after the body has moved does.
///
/// Behaviour: detection.intersect.each-detection-of-a-target-takes-it-again
#[test]
fn each_detection_of_a_target_takes_it_again() {
    let land = land();
    let mut world = table::World::new();
    let mut b = body(50.0, 50.0);
    let key = world.insert_body(&land, WATCHER, &mut b);
    put(&mut world, &land, POST, post(), standing(53.0, 50.0), true);
    let game = After(vec![POST]);
    let mut s = DetectorSettings {
        context: 9,
        ..DetectorSettings::default()
    };
    s.set_shape_sphere(5.0, Vec3::ZERO);
    s.add_target(POST);
    // The way to the post is blocked neither by the post nor by the body itself.
    let mut through = auberean_physics::ethereal::ObjectEthereality::default();
    through.add_to_ethereal_list(POST);
    through.add_to_ethereal_list(WATCHER);
    s.set_detect_offset(
        POST,
        20.0,
        0.0,
        false,
        Vec3::ZERO,
        &through,
        PhysicsTypes::new(0),
        false,
    );
    let found = |b: &mut auberean_physics::body::Body, world: &mut table::World| {
        b.do_intersection(&s, &mut world.detection(&land, key, &game))
            .map(|(f, _)| f.keys().copied().collect::<Vec<_>>())
    };
    assert_eq!(found(&mut b, &mut world), Some(vec![POST]), "the first");
    assert_eq!(
        found(&mut b, &mut world),
        Some(vec![POST]),
        "the second, in the same tick"
    );

    // Taken once a count: the same object taken again with no search or move between is not.
    let post = world.find(POST).expect("the post");
    let mut taken = auberean_physics::detect::Sightings::default();
    world.add_to_intersection_targeted(post, &s, &mut taken, false, &game);
    assert!(taken.is_empty(), "already taken at this count: {taken:?}");
    b.contact = false;
    b.on_walkable = false;
    for _ in 0..2 {
        b.tick_in(&land, &mut world, 0.1);
    }
    world.add_to_intersection_targeted(post, &s, &mut taken, false, &game);
    assert_eq!(
        taken.keys().copied().collect::<Vec<_>>(),
        vec![POST],
        "taken again after the body moved"
    );
}

/// A ray crossing a landblock's east edge is cut there: the part inside the landblock, then the
/// rest in the frame of the landblock to the east; one crossing a corner goes on into the corner
/// landblock; one ending inside its landblock is one piece; a box near an edge reaches the
/// landblocks across it.
///
/// Behaviour: detection.ray.a-ray-is-cut-at-landblock-edges
#[test]
fn a_ray_is_cut_at_landblock_edges() {
    let home = 0x4040_FFFF;
    let ray = Ray::segment(Vec3::new(150.0, 80.0, 10.0), Vec3::new(170.0, 80.0, 10.0));
    let pieces = split_ray(&ray, home);
    assert_eq!(pieces.len(), 2);
    assert_eq!(pieces[0].0, home);
    assert!((pieces[0].1.length - 10.0).abs() < 1e-3);
    assert!((pieces[0].1.end_point().x - 160.0).abs() < 1e-3);
    assert_eq!(pieces[1].0, 0x4140_FFFF);
    assert!(pieces[1].1.point.x.abs() < 1e-3);
    assert!((pieces[1].1.end_point().x - 10.0).abs() < 1e-3);

    let mut rest = Ray::segment(Vec3::new(155.0, 155.0, 0.0), Vec3::new(165.0, 165.0, 0.0));
    let mut piece = Ray::default();
    let mut dir = 0;
    assert!(block_ray_clip(&mut rest, &mut piece, &mut dir));
    assert_eq!(dir, direction::NORTHEAST);
    assert!((rest.end_point().x - 5.0).abs() < 1e-3 && (rest.end_point().y - 5.0).abs() < 1e-3);

    let inside = Ray::segment(Vec3::new(10.0, 10.0, 0.0), Vec3::new(20.0, 30.0, 0.0));
    assert_eq!(split_ray(&inside, home), vec![(home, inside)]);

    let near_east = BBox::new(Vec3::new(150.0, 70.0, 0.0), Vec3::new(165.0, 90.0, 5.0));
    assert_eq!(
        block_list(CellId(0x4040_0031), &near_east),
        vec![home, 0x4140_FFFF]
    );
}

/// The rays covering a sphere from outside it: one to its middle, five at resolution 2, nine at 3,
/// seventeen at 4, the four of resolution 2 reaching its rim across the line of sight; from inside
/// it one ray of no length. A box seen from the west, level with it: one ray to its middle, then
/// one to each corner but the far, north, top one (an origin within the box's span on an axis
/// counts as on its least side, so most corners are taken).
///
/// Behaviour: detection.ray.an-object-is-covered-by-more-rays-as-asked
#[test]
fn an_object_is_covered_by_more_rays_as_asked() {
    let origin = standing(50.0, 50.0);
    let target = standing(60.0, 50.0);
    let ball = dereth_primitives::shape::Sphere::new(Vec3::ZERO, 1.0);
    for (resolution, count) in [(1, 1), (2, 5), (3, 9), (4, 17)] {
        let mut rays = Vec::new();
        sphere_cover_rays(&ball, &target, &origin, resolution, &mut rays);
        assert_eq!(rays.len(), count, "resolution {resolution}");
    }
    let mut rays = Vec::new();
    sphere_cover_rays(&ball, &target, &origin, 2, &mut rays);
    assert!((rays[0].length - 10.0).abs() < 1e-4);
    for r in &rays[1..] {
        let e = r.end_point();
        let off = ((e.x - 60.0).powi(2) + (e.y - 50.0).powi(2) + (e.z - 10.0).powi(2)).sqrt();
        assert!((off - 1.0).abs() < 1e-4, "on the rim: {e:?}");
        assert!((e.x - 60.0).abs() < 1e-4, "across the line: {e:?}");
    }
    let mut rays = Vec::new();
    sphere_cover_rays(&ball, &origin, &origin, 4, &mut rays);
    assert_eq!(rays.len(), 1);
    assert_eq!(rays[0].length, 0.0);

    let cube = BBox::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0));
    let mut rays = Vec::new();
    box_cover_rays(&cube, &target, &origin, 2, &mut rays);
    assert_eq!(rays.len(), 1 + 7);
    let near = |e: Vec3, x: f32, y: f32, z: f32| {
        (e.x - x).abs() < 1e-3 && (e.y - y).abs() < 1e-3 && (e.z - z).abs() < 1e-3
    };
    let ends: Vec<Vec3> = rays.iter().map(Ray::end_point).collect();
    assert!(near(ends[0], 60.0, 50.0, 11.0), "the middle: {:?}", ends[0]);
    assert!(
        ends.iter().any(|e| near(*e, 61.0, 49.0, 10.0)),
        "a far corner"
    );
    assert!(
        !ends.iter().any(|e| near(*e, 61.0, 51.0, 12.0)),
        "not the far top one"
    );
}

/// A detection takes an object with no game object for any kind it reports, unless it ignores the
/// object or has targets the object isn't among.
///
/// Behaviour: detection.world.an-object-with-no-game-object-is-of-every-kind
#[test]
fn an_object_with_no_game_object_is_of_every_kind_a_detection_reports() {
    use auberean_physics::world::NoGameObjects;
    let land = land();
    let mut world = table::World::new();
    let mut m = body(60.0, 50.0);
    m.id = MOVER;
    let mover = world.insert_body(&land, MOVER, &mut m);
    let after = |s: &DetectorSettings| world.check_type_match(mover, s, &NoGameObjects);
    assert!(after(&DetectorSettings::default()), "any kind");
    let ignored = DetectorSettings {
        ignored: vec![MOVER],
        ..DetectorSettings::default()
    };
    assert!(!after(&ignored), "it is ignored");
    let others = DetectorSettings {
        targets: vec![WATCHER],
        ..DetectorSettings::default()
    };
    assert!(!after(&others), "another is the target");
    let it = DetectorSettings {
        targets: vec![MOVER],
        ..DetectorSettings::default()
    };
    assert!(after(&it), "it is the target");
}

/// A body's other updates check its detection spheres against the world it moves in: a sphere
/// around another body notices it once a second has passed and the other has come within reach,
/// and the body's other updates keep running while the sphere is there.
///
/// Behaviour: detection.world.a-bodys-updates-check-its-spheres-against-the-world
#[test]
fn a_bodys_updates_check_its_spheres_against_the_world() {
    use auberean_physics::body::missile::NoObjects;
    use auberean_physics::world::NoGameObjects;
    let land = land();
    let mut world = table::World::new();
    let mut w = body(50.0, 50.0);
    w.id = WATCHER;
    let key = world.insert_body(&land, WATCHER, &mut w);
    let mut m = body(60.0, 50.0);
    m.id = MOVER;
    let mover = world.insert_body(&land, MOVER, &mut m);
    assert!(w.create_detection_cylsphere(
        7,
        MOVER,
        2.0,
        ReportType::default(),
        &mut world.detection(&land, key, &NoGameObjects),
    ));
    let detected = |w: &auberean_physics::body::Body| {
        w.detections
            .as_ref()
            .and_then(|d| d.detectors.get(&7))
            .is_some_and(|d| d.object_detected)
    };
    world.set_now(1.0);
    w.update_in(&land, &mut world, &mut NoObjects, &NoGameObjects, 0.1);
    assert!(!detected(&w), "ten meters off");
    assert!(world.place(&land, mover, standing(51.5, 50.0)));
    world.set_now(1.5);
    w.update_in(&land, &mut world, &mut NoObjects, &NoGameObjects, 0.1);
    assert!(!detected(&w), "not a second since the last check");
    world.set_now(2.1);
    w.update_in(&land, &mut world, &mut NoObjects, &NoGameObjects, 0.1);
    assert!(detected(&w), "within reach");
    assert!(w.is_misc_active());
}

mod rooms {
    //! Lines of sight inside: a room's own walls (the solid side of its physics tree), what stands in
    //! it, and its doorways into the room beyond or out onto the land.
    //! Fixture: made-up rooms of the flat 160 m block at height 10 of the objects tests: a room with a
    //! doorway north into a second room, and rooms with a doorway east out onto the land; the objects
    //! tests' upright posts, 1 m across.

    use std::sync::Arc;

    use auberean_physics::cells::OUTDOORS;
    use auberean_physics::ethereal::SightLine;
    use auberean_physics::geom::Ray;
    use auberean_physics::land::Land;
    use auberean_physics::world::{self as table, WorldObject};
    use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Polygon, Sphere};
    use dereth_physics::source::{CellPortal, EnvCellGeometry};
    use dereth_primitives::shape::Plane;
    use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

    use crate::contact::objects::{at as standing, land, post};

    /// The room the lines start in.
    const NEAR: CellId = CellId(0x4040_0100);
    /// The room beyond its doorway.
    const FAR: CellId = CellId(0x4040_0101);
    /// A room whose doorway out lies over the land cell its middle does.
    const SHED: CellId = CellId(0x4040_0102);
    /// A room whose doorway out lies over another land cell than its middle does.
    const HUT: CellId = CellId(0x4040_0103);
    const POST: u64 = 0x7000_0001;

    fn leaf(index: i32, solid: bool) -> BspNode {
        BspNode {
            sphere: Sphere::new(Vec3::ZERO, 100.0),
            splitting_plane: Plane::default(),
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Leaf {
                leaf_index: index,
                solid,
            },
            in_polys: Vec::new(),
        }
    }

    /// A tree open on the front of the plane `normal · p + d = 0` and solid behind it.
    fn solid_behind(normal: Vec3, d: f32) -> BspTree {
        BspTree {
            nodes: vec![
                BspNode {
                    sphere: Sphere::new(Vec3::ZERO, 100.0),
                    splitting_plane: Plane { normal, d },
                    pos_child: Some(1),
                    neg_child: Some(2),
                    kind: BspNodeKind::Node,
                    in_polys: Vec::new(),
                },
                leaf(0, false),
                leaf(1, true),
            ],
            polygons: Vec::new(),
        }
    }

    /// A tree open everywhere.
    fn open() -> BspTree {
        BspTree {
            nodes: vec![leaf(0, false)],
            polygons: Vec::new(),
        }
    }

    /// A doorway 2 m across and 2 m high around `middle`, in the plane `y = middle.y` (`across_y`)
    /// or `x = middle.x`, leading to `other` by its doorway `back`.
    fn doorway(middle: Vec3, across_y: bool, other: u32, back: i32) -> CellPortal {
        let (m, k) = (middle, 1.0);
        let corners = if across_y {
            vec![
                Vec3::new(m.x - k, m.y, m.z - k),
                Vec3::new(m.x + k, m.y, m.z - k),
                Vec3::new(m.x + k, m.y, m.z + k),
                Vec3::new(m.x - k, m.y, m.z + k),
            ]
        } else {
            vec![
                Vec3::new(m.x, m.y - k, m.z - k),
                Vec3::new(m.x, m.y + k, m.z - k),
                Vec3::new(m.x, m.y + k, m.z + k),
                Vec3::new(m.x, m.y - k, m.z + k),
            ]
        };
        CellPortal {
            other_cell_id: other,
            portal: Polygon::new(corners),
            portal_side: false,
            other_portal_id: back,
            exact_match: false,
        }
    }

    /// Room `id`, its frame the landblock's, with physics tree `tree` and doorways `portals`.
    fn room(land: &Land, id: CellId, tree: BspTree, portals: Vec<CellPortal>) {
        land.insert_cell(EnvCellGeometry {
            id,
            frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
            portals,
            physics_polygons: Vec::new(),
            physics_bsp: Some(Arc::new(tree)),
            cell_bsp: None,
            stab_list: Vec::new(),
            seen_outside: false,
            holds_without_portals: true,
            static_objects: Vec::new(),
        });
    }

    /// The near room (its own north wall at `y = 15`, a doorway north at `y = 5`) and the far room
    /// beyond it (its north wall at `y = 12`).
    fn two_rooms(land: &Land) {
        let door = Vec3::new(0.0, 5.0, 0.0);
        room(
            land,
            NEAR,
            solid_behind(Vec3::new(0.0, -1.0, 0.0), 15.0),
            vec![doorway(door, true, FAR.0, 0)],
        );
        room(
            land,
            FAR,
            solid_behind(Vec3::new(0.0, -1.0, 0.0), 12.0),
            vec![doorway(door, true, NEAR.0, 0)],
        );
    }

    /// A line of sight from `from` to `to` in `cell`, tracing `intersect`.
    fn sight(cell: CellId, from: Vec3, to: Vec3, intersect: u32) -> SightLine {
        let mut los = SightLine::default();
        los.set_ray(Ray::segment(from, to), cell.0);
        los.set_intersect_type(intersect);
        los
    }

    fn traced(world: &table::World, land: &Land, mut los: SightLine) -> SightLine {
        assert!(world.los_check(land, &mut los), "a loaded first cell");
        los
    }

    /// A line of sight goes on through a room's doorway into the room beyond, and that room's own wall
    /// stops it there, facing back; a shorter one ends in the far room, whole. Through no doorway, the
    /// near room's own wall stops it. Tracing doorways only, it stops at the doorway in the near
    /// room; tracing doorways into rooms, it goes on into the far room and ends there, whole (no wall
    /// counts). A line told not to go through doorways ends at the near room's own wall. Something
    /// standing in the far room stops it before the wall.
    ///
    /// Behaviour: detection.sight.a-line-of-sight-goes-through-doorways-into-the-rooms-beyond
    #[test]
    fn a_line_of_sight_goes_through_doorways_into_the_rooms_beyond() {
        let land = land();
        two_rooms(&land);
        let mut world = table::World::new();
        let (from, to) = (Vec3::ZERO, Vec3::new(0.0, 20.0, 0.0));

        let los = traced(&world, &land, sight(NEAR, from, to, 0));
        assert_eq!(los.trace_result, 1, "stopped");
        assert_eq!(los.final_cell, FAR.0, "in the far room");
        assert!(
            (los.end_point().y - 12.0).abs() < 1e-3,
            "{:?}",
            los.end_point()
        );
        assert!(
            (los.impact_normal.y + 1.0).abs() < 1e-6,
            "facing back: {:?}",
            los.impact_normal
        );

        let los = traced(
            &world,
            &land,
            sight(NEAR, from, Vec3::new(0.0, 10.0, 0.0), 0),
        );
        assert_eq!(
            (los.trace_result, los.final_cell),
            (0, FAR.0),
            "whole, beyond"
        );
        assert!((los.end_point().y - 10.0).abs() < 1e-3);

        let beside = traced(
            &world,
            &land,
            sight(NEAR, Vec3::new(3.0, 0.0, 0.0), Vec3::new(3.0, 20.0, 0.0), 0),
        );
        assert_eq!((beside.trace_result, beside.final_cell), (1, NEAR.0));
        assert!((beside.end_point().y - 15.0).abs() < 1e-3, "the near wall");

        let doors = traced(&world, &land, sight(NEAR, from, to, 3));
        assert_eq!((doors.trace_result, doors.final_cell), (1, NEAR.0));
        assert_eq!(doors.portal_index, 0);
        assert!((doors.end_point().y - 5.0).abs() < 1e-3, "at the doorway");
        assert!((doors.impact_normal.y.abs() - 1.0).abs() < 1e-6);

        let rooms = traced(&world, &land, sight(NEAR, from, to, 4));
        assert_eq!((rooms.trace_result, rooms.final_cell), (0, FAR.0));
        assert!((rooms.end_point().y - 20.0).abs() < 1e-3);

        let mut stay = sight(NEAR, from, to, 0);
        stay.traverse_portals = false;
        let stay = traced(&world, &land, stay);
        assert_eq!((stay.trace_result, stay.final_cell), (1, NEAR.0));
        assert!((stay.end_point().y - 15.0).abs() < 1e-3);

        world.insert(
            &land,
            POST,
            &WorldObject {
                shape: post(),
                position: Position::new(FAR, Frame::new(Vec3::new(0.0, 8.0, -1.0), Quat::IDENTITY)),
                is_static: true,
                passable: false,
            },
        );
        let los = traced(&world, &land, sight(NEAR, from, to, 0));
        assert_eq!((los.trace_result, los.final_cell), (1, FAR.0));
        assert!(
            (los.end_point().y - 7.5).abs() < 0.01,
            "at the post: {:?}",
            los.end_point()
        );
        assert_eq!(los.impact_normal, Vec3::ZERO, "an object faces nowhere");
    }

    /// Room `id` standing at `middle`, with its tree's sphere of radius `radius` around the room's
    /// own middle and one doorway out, east of it, 5 m off; `wall` a solid wall `west` of the middle.
    fn shed_at(land: &Land, id: CellId, middle: Vec3, radius: f32, west: f32) {
        let mut tree = solid_behind(Vec3::new(1.0, 0.0, 0.0), west);
        tree.nodes[0].sphere = Sphere::new(Vec3::ZERO, radius);
        land.insert_cell(EnvCellGeometry {
            id,
            frame: Frame::new(middle, Quat::IDENTITY),
            portals: vec![doorway(Vec3::new(5.0, 0.0, 0.0), false, OUTDOORS, -1)],
            physics_polygons: Vec::new(),
            physics_bsp: Some(Arc::new(tree)),
            cell_bsp: None,
            stab_list: Vec::new(),
            seen_outside: false,
            holds_without_portals: true,
            static_objects: Vec::new(),
        });
    }

    /// A room with a doorway out and a physics tree touches the land cells under the box around the
    /// tree's sphere, as the block names it among the cells it sees from outside: a room the block
    /// does not name, one with no doorway out and one with no tree touch nothing. A land cell past
    /// the block's edge counts as the cell at the same place in this block.
    ///
    /// Behaviour: detection.sight.a-room-with-a-doorway-out-touches-the-land-cells-under-its-sphere
    #[test]
    fn a_room_with_a_doorway_out_touches_the_land_cells_under_its_sphere() {
        let land = land();
        shed_at(&land, SHED, Vec3::new(58.0, 50.0, 11.0), 5.0, 100.0);
        // Over the block's east edge; its tree's sphere reaches the block beside this one.
        shed_at(&land, HUT, Vec3::new(158.0, 50.0, 11.0), 5.0, 100.0);
        // Named, with no doorway out.
        room(
            &land,
            NEAR,
            open(),
            vec![doorway(Vec3::ZERO, true, FAR.0, 0)],
        );
        // Named, a doorway out, no tree.
        land.insert_cell(EnvCellGeometry {
            id: FAR,
            frame: Frame::new(Vec3::new(50.0, 50.0, 11.0), Quat::IDENTITY),
            portals: vec![doorway(Vec3::ZERO, true, OUTDOORS, -1)],
            physics_polygons: Vec::new(),
            physics_bsp: None,
            cell_bsp: None,
            stab_list: Vec::new(),
            seen_outside: false,
            holds_without_portals: true,
            static_objects: Vec::new(),
        });
        // Not named.
        shed_at(
            &land,
            CellId(0x4040_0104),
            Vec3::new(50.0, 50.0, 11.0),
            5.0,
            100.0,
        );
        land.connect_cells(0x40, 0x40, &[0x100, 0x101, 0x102, 0x103]);

        let cell = |x: f32, y: f32| standing(x, y).cell;
        assert_eq!(land.touching_cells(cell(50.0, 50.0)), vec![SHED]);
        assert_eq!(land.touching_cells(cell(61.0, 50.0)), vec![SHED]);
        assert_eq!(land.touching_cells(cell(150.0, 50.0)), vec![HUT]);
        assert_eq!(
            land.touching_cells(cell(5.0, 50.0)),
            vec![HUT],
            "past the edge, the same place in this block"
        );
        assert!(land.touching_cells(cell(50.0, 90.0)).is_empty());
    }

    /// A line of sight over the land asks the rooms that touch the land cells it crosses: going in
    /// through a room's doorway out, it goes on inside, and the room's wall stops it there, in the
    /// room, facing back. Past the doorway's side, or with the room touching no cell it crosses, it
    /// goes on over the land, clear. A line that the ground stops before the doorway never reaches it.
    ///
    /// Behaviour: detection.sight.a-line-of-sight-over-the-land-goes-in-through-the-doorways-of-rooms-it-touches
    #[test]
    fn a_line_of_sight_over_the_land_goes_in_through_the_doorways_of_rooms_it_touches() {
        let land = land();
        shed_at(&land, SHED, Vec3::new(50.0, 50.0, 11.0), 5.0, 8.0);
        land.connect_cells(0x40, 0x40, &[0x102]);
        let world = table::World::new();
        let start = standing(58.0, 50.0).cell;

        let los = traced(
            &world,
            &land,
            sight(
                start,
                Vec3::new(58.0, 50.0, 11.0),
                Vec3::new(30.0, 50.0, 11.0),
                0,
            ),
        );
        assert_eq!((los.trace_result, los.final_cell), (1, SHED.0), "{los:?}");
        assert!(
            (los.end_point().x - 42.0).abs() < 1e-3,
            "at the wall, 8 m west of the room's middle: {:?}",
            los.end_point()
        );
        assert!((los.impact_normal.x - 1.0).abs() < 1e-6);

        let beside = traced(
            &world,
            &land,
            sight(
                start,
                Vec3::new(58.0, 55.0, 11.0),
                Vec3::new(30.0, 55.0, 11.0),
                0,
            ),
        );
        assert_eq!(beside.trace_result, 0, "{beside:?}");
        assert_eq!(beside.final_cell, standing(30.0, 55.0).cell.0);

        let short = traced(
            &world,
            &land,
            sight(
                start,
                Vec3::new(58.0, 50.0, 11.0),
                Vec3::new(50.0, 50.0, 5.0),
                0,
            ),
        );
        assert_eq!(short.trace_result, 1, "{short:?}");
        assert_eq!(short.final_cell, standing(55.0, 50.0).cell.0, "on the land");
        assert!(short.end_point().z >= 9.99, "{:?}", short.end_point());

        // The same room, told of nothing.
        let land = crate::contact::objects::land();
        shed_at(&land, SHED, Vec3::new(50.0, 50.0, 11.0), 5.0, 8.0);
        let los = traced(
            &world,
            &land,
            sight(
                start,
                Vec3::new(58.0, 50.0, 11.0),
                Vec3::new(30.0, 50.0, 11.0),
                0,
            ),
        );
        assert_eq!(los.trace_result, 0);
        assert_eq!(los.final_cell, standing(30.0, 50.0).cell.0);
    }

    /// The land stops a line of sight at the first of its triangles the line crosses, either way, in
    /// the land cell the triangle is in, and the line's impact faces the way the ground does. A line
    /// straight down stops the same way.
    ///
    /// Behaviour: detection.sight.the-land-stops-a-line-of-sight-at-its-triangles-facing-up
    #[test]
    fn the_land_stops_a_line_of_sight_at_its_triangles_facing_up() {
        let land = land();
        let world = table::World::new();
        let start = standing(39.0, 50.0).cell;
        for (from, to, end) in [
            // Down, across a land cell's edge.
            (
                Vec3::new(38.0, 50.0, 11.0),
                Vec3::new(42.0, 50.0, 9.0),
                Vec3::new(40.0, 50.0, 10.0),
            ),
            // Up, from under the ground.
            (
                Vec3::new(38.0, 50.0, 9.0),
                Vec3::new(38.0, 54.0, 11.0),
                Vec3::new(38.0, 52.0, 10.0),
            ),
            // Straight down.
            (
                Vec3::new(38.0, 50.0, 12.0),
                Vec3::new(38.0, 50.0, 8.0),
                Vec3::new(38.0, 50.0, 10.0),
            ),
        ] {
            let los = traced(&world, &land, sight(start, from, to, 0));
            assert_eq!(los.trace_result, 1, "{from:?}: {los:?}");
            let e = los.end_point();
            assert!(
                (e.x - end.x).abs() < 1e-2
                    && (e.y - end.y).abs() < 1e-2
                    && (e.z - end.z).abs() < 1e-3,
                "{e:?}"
            );
            assert!((los.impact_normal.z - 1.0).abs() < 1e-6, "{los:?}");
        }
    }

    /// A line of sight tracing doorways only passes what stands on the land; it passes nothing else's
    /// test either way.
    ///
    /// Behaviour: detection.sight.a-line-tracing-doorways-passes-what-stands-on-the-land
    #[test]
    fn a_line_tracing_doorways_passes_what_stands_on_the_land() {
        let land = land();
        let mut world = table::World::new();
        world.insert(
            &land,
            POST,
            &WorldObject {
                shape: post(),
                position: standing(60.0, 50.0),
                is_static: true,
                passable: false,
            },
        );
        let start = standing(50.0, 50.0).cell;
        let (from, to) = (Vec3::new(50.0, 50.0, 11.0), Vec3::new(70.0, 50.0, 11.0));
        let los = traced(&world, &land, sight(start, from, to, 0));
        assert_eq!(los.trace_result, 1, "the post stops a line of shapes");
        assert!(
            (los.impact_normal.x + 1.0).abs() < 1e-6,
            "facing back along the line: {:?}",
            los.impact_normal
        );
        let los = traced(&world, &land, sight(start, from, to, 3));
        assert_eq!(los.trace_result, 0, "a line of doorways passes it: {los:?}");
    }

    /// What stands in a cell and a landblock is asked for by its own cell, the static left out when
    /// only what moves is asked for; an object knows how many cells' lists it is in, whether each is
    /// loaded, and with something holding it its lists are emptied.
    ///
    /// Behaviour: detection.cells.a-cell-and-its-landblock-answer-for-what-stands-in-them
    #[test]
    fn a_cell_and_its_landblock_answer_for_what_stands_in_them() {
        let land = land();
        let mut world = table::World::new();
        let key = world.insert(
            &land,
            POST,
            &WorldObject {
                shape: post(),
                position: standing(50.0, 50.0),
                is_static: true,
                passable: false,
            },
        );
        let cell = standing(50.0, 50.0).cell;
        assert_eq!(world.cell_objects(cell, false), vec![POST]);
        assert!(
            world.cell_objects(cell, true).is_empty(),
            "it does not move"
        );
        assert_eq!(world.block_objects(&land, cell, false), vec![POST]);
        assert!(world
            .cell_objects(standing(10.0, 10.0).cell, false)
            .is_empty());
        assert_eq!(world.num_shadow_objects(key), 1);
        assert!(world.are_shadow_objects_complete(&land, key));
        world.recalc_cross_cells(&land, key, true);
        assert_eq!(world.num_shadow_objects(key), 0, "held: in no list");
        assert!(!world.are_shadow_objects_complete(&land, key));
        world.recalc_cross_cells(&land, key, false);
        assert_eq!(world.num_shadow_objects(key), 1, "standing again");
    }

    /// A point lying in an interior cell finds that cell: the lowest id of those that hold it; a
    /// point in none finds none.
    ///
    /// Behaviour: detection.cells.a-point-finds-the-interior-cell-holding-it
    #[test]
    fn a_point_finds_the_interior_cell_holding_it() {
        let land = land();
        // Each holds what lies behind its wall (the tree's solid side).
        let walled = |id: CellId, d: f32| EnvCellGeometry {
            id,
            frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
            portals: Vec::new(),
            physics_polygons: Vec::new(),
            physics_bsp: None,
            cell_bsp: Some(Arc::new(solid_behind(Vec3::new(1.0, 0.0, 0.0), d))),
            stab_list: Vec::new(),
            seen_outside: false,
            holds_without_portals: true,
            static_objects: Vec::new(),
        };
        land.insert_cell(walled(FAR, -10.0));
        land.insert_cell(walled(NEAR, -20.0));
        let holds = |x: f32| {
            let t = solid_behind(Vec3::new(1.0, 0.0, 0.0), -10.0);
            t.point_inside_cell_bsp(Vec3::new(x, 0.0, 0.0))
        };
        let (inside, outside) = if holds(5.0) { (5.0, 40.0) } else { (40.0, 5.0) };
        assert_eq!(
            land.find_in_visible_cells(Vec3::new(inside, 0.0, 0.0)),
            Some(NEAR),
            "the lowest id"
        );
        let beyond = land.find_in_visible_cells(Vec3::new(outside, 0.0, 0.0));
        assert_ne!(beyond, Some(FAR), "{beyond:?}");
    }

    /// A line of sight leaving a room through a doorway out goes on over the land from the doorway,
    /// and a post standing outside stops it. When the line's start
    /// lies over another land cell than the doorway does, the land traces the line again from its
    /// start, so a post standing between the start and the doorway stops it too.
    ///
    /// Behaviour: detection.sight.a-line-of-sight-goes-out-through-a-doorway-onto-the-land
    #[test]
    fn a_line_of_sight_goes_out_through_a_doorway_onto_the_land() {
        let land = land();
        room(
            &land,
            SHED,
            open(),
            vec![doorway(Vec3::new(55.0, 50.0, 11.0), false, OUTDOORS, -1)],
        );
        room(
            &land,
            HUT,
            open(),
            vec![doorway(Vec3::new(61.0, 50.0, 11.0), false, OUTDOORS, -1)],
        );
        let mut world = table::World::new();
        let shed = (Vec3::new(50.0, 50.0, 11.0), Vec3::new(70.0, 50.0, 11.0));

        let los = traced(&world, &land, sight(SHED, shed.0, shed.1, 0));
        assert_eq!(los.trace_result, 0, "clear: {los:?}");
        assert_eq!(
            los.final_cell,
            standing(70.0, 50.0).cell.0,
            "ends over the land"
        );

        world.insert(
            &land,
            POST,
            &WorldObject {
                shape: post(),
                position: standing(60.0, 50.0),
                is_static: true,
                passable: false,
            },
        );
        let los = traced(&world, &land, sight(SHED, shed.0, shed.1, 0));
        assert_eq!(los.trace_result, 1, "stopped outside");
        assert_eq!(
            los.final_cell,
            standing(70.0, 50.0).cell.0,
            "reported in the last land cell of the box around the line"
        );
        assert!(
            (los.end_point().x - 59.5).abs() < 0.01,
            "{:?}",
            los.end_point()
        );

        // The hut's doorway at x = 61 lies over the next land cell east; its start does not.
        let before = sight(
            HUT,
            Vec3::new(56.0, 50.0, 11.0),
            Vec3::new(58.0, 50.0, 11.0),
            0,
        );
        let before = traced(&world, &land, before);
        assert_eq!(
            (before.trace_result, before.final_cell),
            (0, HUT.0),
            "short of the doorway"
        );
        let los = sight(
            HUT,
            Vec3::new(56.0, 50.0, 11.0),
            Vec3::new(75.0, 50.0, 11.0),
            0,
        );
        let los = traced(&world, &land, los);
        assert_eq!(
            los.trace_result, 1,
            "stopped by the post behind the doorway: {los:?}"
        );
        assert!(
            (los.end_point().x - 59.5).abs() < 0.01,
            "{:?}",
            los.end_point()
        );
    }
}
