//! Detection: volumes a body sets around itself (or around a target) that notice the objects inside
//! them, the settings of such a volume, what is known of each object it notices, and the rays a
//! line-of-sight check casts through each landblock.

use std::collections::BTreeMap;

use auberean_assets::Context;
use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::ethereal::{ObjectEthereality, PhysicsTypes, SightLine};
use crate::geom::{BBox, BoxOps, Cone, CylinderOps, PlaneCone, Ray, SphereOps};
use crate::pack::{
    read_ids, read_packs, read_position, read_vec3, write_ids, write_packs, write_position,
    write_table_header, write_vec3, Pack, Writer,
};
use dereth_primitives::shape::{CylSphere, Sphere};

pub mod manager;

/// The kinds of object a volume reports, one bit each.
// D2-15715
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportType {
    pub mask: u64,
}

impl Pack for ReportType {
    fn pack(&self, w: &mut Writer) {
        w.u64(self.mask);
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        Ok(Self { mask: r.u64()? })
    }
}

fn origin() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

/// The shape of a detection volume, by its stored kind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Shape {
    /// Kind 0: none.
    #[default]
    None,
    /// Kind 1.
    Ray(Ray),
    /// Kind 2: a point, which holds nothing.
    Point(Vec3),
    /// Kind 3.
    Cone(Cone),
    /// Kind 4.
    Ball(Sphere),
    /// Kind 5.
    PlaneCone(PlaneCone),
    /// Kind 6.
    Box(BBox),
    /// Kind 7.
    Cylinder(CylSphere),
}

impl Shape {
    #[must_use]
    pub fn kind(&self) -> u32 {
        match self {
            Self::None => 0,
            Self::Ray(_) => 1,
            Self::Point(_) => 2,
            Self::Cone(_) => 3,
            Self::Ball(_) => 4,
            Self::PlaneCone(_) => 5,
            Self::Box(_) => 6,
            Self::Cylinder(_) => 7,
        }
    }
}

/// A detection volume: where it is, and its shape there.
// D2-15611, D2-15605
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    pub position: Position,
    pub shape: Shape,
}

impl Default for Volume {
    // D2-15591
    fn default() -> Self {
        Self {
            position: origin(),
            shape: Shape::None,
        }
    }
}

impl Volume {
    /// The box around the volume's shape in the landblock frame of its own cell; none for a volume
    /// with no cell, no shape, a ray or a point.
    // D2-15592
    #[must_use]
    pub fn global_box(&self) -> Option<BBox> {
        let p = &self.position;
        if p.cell.0 == 0 {
            return None;
        }
        match &self.shape {
            Shape::Cone(c) => Some(Sphere::new(c.center, c.radius).bounding_box(p, p)),
            Shape::Ball(b) => Some(b.bounding_box(p, p)),
            Shape::PlaneCone(c) => Some(c.bounding_box(p, p)),
            Shape::Box(b) => Some(b.local_to_global(p, p)),
            Shape::Cylinder(c) => Some(CylinderOps::bounding_box(c, p, p)),
            Shape::None | Shape::Ray(_) | Shape::Point(_) => None,
        }
    }

    /// Whether a sphere at `at` reaches the volume. The sphere is first scaled by `scale` when its
    /// three parts are equal (else left as it is), then carried into the volume's frame.
    // D2-15598
    #[must_use]
    pub fn is_within_sphere(&self, s: &Sphere, at: &Position, scale: Vec3) -> bool {
        let k = if scale.x != scale.y || scale.x != scale.z {
            1.0
        } else {
            scale.z
        };
        let c = Vec3::new(k * s.center.x, k * s.center.y, k * s.center.z);
        let s = Sphere::new(
            crate::geom::point_local_to_local(&self.position, at, c),
            k * s.radius,
        );
        match &self.shape {
            Shape::Cone(c) => c.intersects_sphere(&s),
            Shape::Ball(b) => {
                let (dx, dy, dz) = (
                    b.center.x - s.center.x,
                    b.center.y - s.center.y,
                    b.center.z - s.center.z,
                );
                let r = s.radius + b.radius;
                (dx * dx + dy * dy + dz * dz) - r * r < crate::geom::EPS
            }
            Shape::PlaneCone(c) => c.intersects_sphere(&s),
            Shape::Box(b) => b.intersects_sphere(&s),
            Shape::Cylinder(c) => c.intersects_sphere(&s),
            Shape::None | Shape::Ray(_) | Shape::Point(_) => false,
        }
    }

    /// Whether an upright cylinder at `at` reaches the volume, its base carried into the
    /// volume's frame.
    // D2-15599
    #[must_use]
    pub fn is_within_cylinder(&self, c: &CylSphere, at: &Position) -> bool {
        let c = CylSphere {
            low_pt: crate::geom::point_local_to_local(&self.position, at, c.low_pt),
            ..*c
        };
        match &self.shape {
            Shape::Cone(k) => k.intersects_cylinder(&c),
            Shape::Ball(b) => b.intersects_cylinder(&c, false),
            Shape::PlaneCone(k) => k.intersects_cylinder(&c),
            Shape::Box(b) => b.intersects_cylinder(&c),
            Shape::Cylinder(own) => own.intersects_cylinder(&c),
            Shape::None | Shape::Ray(_) | Shape::Point(_) => false,
        }
    }

    /// Whether a box at `at` reaches the volume: its two corners carried into the volume's frame
    /// (not the box around its corners there).
    // D2-15600
    #[must_use]
    pub fn is_within_box(&self, b: &BBox, at: &Position) -> bool {
        let b = BBox::new(
            crate::geom::point_local_to_local(&self.position, at, b.min),
            crate::geom::point_local_to_local(&self.position, at, b.max),
        );
        match &self.shape {
            Shape::Cone(k) => k.intersects_box(&b),
            Shape::Ball(s) => b.intersects_sphere(s),
            Shape::PlaneCone(k) => k.intersects_box(&b),
            Shape::Box(own) => own.does_overlap(&b, 0.0),
            Shape::Cylinder(c) => b.intersects_cylinder(c),
            Shape::None | Shape::Ray(_) | Shape::Point(_) => false,
        }
    }

    /// A box spanning from `(x0, y0, z0)` to `(-x1, -y1, -z1)`.
    // D2-15606
    pub fn create_box(&mut self, y0: f32, y1: f32, x0: f32, x1: f32, z0: f32, z1: f32) {
        let p = Vec3::new(x0, y0, z0);
        let mut b = BBox::new(p, p);
        b.adjust_bbox(Vec3::new(-x1, -y1, -z1));
        self.shape = Shape::Box(b);
    }

    // D2-15607
    pub fn create_cylinder(&mut self, radius: f32, low_point: Vec3, height: f32) {
        self.shape = Shape::Cylinder(CylSphere {
            low_pt: low_point,
            radius,
            height,
        });
    }

    // D2-15608
    pub fn create_sphere(&mut self, radius: f32, center: Vec3) {
        self.shape = Shape::Ball(Sphere::new(center, radius));
    }

    // D2-15609
    pub fn create_sphere_cone(&mut self, center: Vec3, radius: f32, dir: Vec3, angle: f32) {
        self.shape = Shape::Cone(Cone::new(center, radius, dir, angle));
    }

    // D2-15610
    #[allow(clippy::too_many_arguments)]
    pub fn create_sphere_plane_cone(
        &mut self,
        center: Vec3,
        min_radius: f32,
        max_radius: f32,
        min_theta: f32,
        max_theta: f32,
        min_phi: f32,
        max_phi: f32,
    ) {
        self.shape = Shape::PlaneCone(PlaneCone::new(
            center, min_radius, max_radius, min_theta, max_theta, min_phi, max_phi,
        ));
    }

    /// Makes `to` this volume. A point volume copies as the zero point and fails, as retail's does;
    /// so does a volume with no shape.
    // D2-15604
    pub fn copy_into(&self, to: &mut Self) -> bool {
        to.position = self.position;
        match self.shape {
            Shape::None => {
                to.shape = Shape::None;
                false
            }
            Shape::Point(_) => {
                to.shape = Shape::Point(Vec3::ZERO);
                false
            }
            s => {
                to.shape = s;
                true
            }
        }
    }
}

fn pack_shape(s: &Shape, w: &mut Writer) {
    match s {
        Shape::None => {}
        Shape::Ray(r) => r.pack(w),
        // A point streams nothing, and the stream fails there (see `Volume::unpack`).
        Shape::Point(_) => {}
        // D2-13379
        Shape::Cone(c) => {
            write_vec3(w, c.center);
            w.f32(c.radius);
            write_vec3(w, c.dir);
            w.f32(c.angle);
            w.f32(c.cos_angle);
        }
        Shape::Ball(b) => {
            write_vec3(w, b.center);
            w.f32(b.radius);
        }
        // D2-13388
        Shape::PlaneCone(c) => {
            write_vec3(w, c.center);
            for v in [
                c.min_radius,
                c.max_radius,
                c.min_theta,
                c.max_theta,
                c.min_phi,
                c.max_phi,
                c.min_z,
                c.max_z,
            ] {
                w.f32(v);
            }
        }
        Shape::Box(b) => {
            write_vec3(w, b.min);
            write_vec3(w, b.max);
        }
        // D2-13153
        Shape::Cylinder(c) => {
            write_vec3(w, c.low_pt);
            w.f32(c.radius);
            w.f32(c.height);
        }
    }
}

fn unpack_shape(kind: u32, r: &mut Reader<'_>, ctx: &Context) -> Result<Shape> {
    Ok(match kind {
        1 => Shape::Ray(Ray::unpack(r, ctx)?),
        2 => return r.fail("a point volume cannot be read"),
        3 => Shape::Cone(Cone {
            center: read_vec3(r)?,
            radius: r.f32()?,
            dir: read_vec3(r)?,
            angle: r.f32()?,
            cos_angle: r.f32()?,
        }),
        4 => Shape::Ball(Sphere::new(read_vec3(r)?, r.f32()?)),
        5 => Shape::PlaneCone(PlaneCone {
            center: read_vec3(r)?,
            min_radius: r.f32()?,
            max_radius: r.f32()?,
            min_theta: r.f32()?,
            max_theta: r.f32()?,
            min_phi: r.f32()?,
            max_phi: r.f32()?,
            min_z: r.f32()?,
            max_z: r.f32()?,
        }),
        6 => Shape::Box(BBox::new(read_vec3(r)?, read_vec3(r)?)),
        7 => {
            let low_pt = read_vec3(r)?;
            let radius = r.f32()?;
            let height = r.f32()?;
            Shape::Cylinder(CylSphere {
                low_pt,
                radius,
                height,
            })
        }
        _ => Shape::None,
    })
}

impl Pack for Volume {
    /// The position, the kind, then the shape; an unknown kind stores nothing more. A point
    /// volume has no binary form: packing one writes the position and the kind only, and reading
    /// one fails after them, as retail's stream gives up on a point.
    // D2-15601, D2-15602, D2-15603, D2-15595, D2-15594
    fn pack(&self, w: &mut Writer) {
        write_position(w, &self.position);
        w.u32(self.shape.kind());
        pack_shape(&self.shape, w);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let position = read_position(r)?;
        let kind = r.u32()?;
        Ok(Self {
            position,
            shape: unpack_shape(kind, r, ctx)?,
        })
    }
}

/// What a detector knows of the object it detected: the object and its status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Detected {
    pub object: u64,
    pub status: u32,
}

/// A detection sphere a body keeps: its context, its radius, whether it has detected its object,
/// what it knows of the object, and the kinds of object it reports.
// D2-15567
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Detector {
    pub context: u32,
    pub radius: f32,
    pub object_detected: bool,
    pub info: Detected,
    pub report: ReportType,
}

impl Detector {
    // D2-06871, D2-06796
    #[must_use]
    pub fn new(context: u32, object: u64, radius: f32, report: ReportType) -> Self {
        Self {
            context,
            radius,
            object_detected: false,
            info: Detected { object, status: 0 },
            report,
        }
    }
}

impl Pack for Detector {
    /// The object, the radius, the context, whether detected, and the report kinds; the object's
    /// status is not sent.
    // D2-06870
    fn pack(&self, w: &mut Writer) {
        w.u64(self.info.object);
        w.f32(self.radius);
        w.u32(self.context);
        w.bool32(self.object_detected);
        self.report.pack(w);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let object = r.u64()?;
        Ok(Self {
            radius: r.f32()?,
            context: r.u32()?,
            object_detected: r.bool32()?,
            info: Detected { object, status: 0 },
            report: ReportType::unpack(r, ctx)?,
        })
    }
}

/// What a detection knows of one object it noticed: its kind, where it is, its id and entity,
/// how much of it a line of sight found blocked, its distance, and four words and four numbers the
/// game logic keeps with it.
// D2-15613, D2-15622, D2-04258
#[derive(Clone, Debug, PartialEq)]
pub struct Sighting {
    pub report: ReportType,
    pub position: Position,
    pub object: u64,
    pub entity: u32,
    pub los_coverage: f32,
    pub distance: f32,
    pub int_app: [u32; 4],
    pub float_app: [f32; 4],
}

impl Default for Sighting {
    // D2-15620
    fn default() -> Self {
        Self {
            report: ReportType::default(),
            position: origin(),
            object: 0,
            entity: 0,
            los_coverage: 0.0,
            distance: 0.0,
            int_app: [0; 4],
            float_app: [0.0; 4],
        }
    }
}

impl Sighting {
    // D2-15614
    pub fn init(&mut self, object: u64) {
        self.object = object;
    }

    // D2-15615
    #[must_use]
    pub fn int_app(&self, i: usize) -> u32 {
        self.int_app[i]
    }

    pub fn set_int_app(&mut self, i: usize, v: u32) {
        self.int_app[i] = v;
    }

    // D2-15617
    #[must_use]
    pub fn float_app(&self, i: usize) -> f32 {
        self.float_app[i]
    }

    pub fn set_float_app(&mut self, i: usize, v: f32) {
        self.float_app[i] = v;
    }
}

impl Pack for Sighting {
    // D2-15621
    fn pack(&self, w: &mut Writer) {
        self.report.pack(w);
        write_position(w, &self.position);
        w.u64(self.object);
        w.u32(self.entity);
        w.f32(self.los_coverage);
        w.f32(self.distance);
        for v in self.int_app {
            w.u32(v);
        }
        for v in self.float_app {
            w.f32(v);
        }
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let report = ReportType::unpack(r, ctx)?;
        let position = read_position(r)?;
        let object = r.u64()?;
        let entity = r.u32()?;
        let los_coverage = r.f32()?;
        let distance = r.f32()?;
        let mut int_app = [0; 4];
        for v in &mut int_app {
            *v = r.u32()?;
        }
        let mut float_app = [0.0; 4];
        for v in &mut float_app {
            *v = r.f32()?;
        }
        Ok(Self {
            report,
            position,
            object,
            entity,
            los_coverage,
            distance,
            int_app,
            float_app,
        })
    }
}

/// The sightings of one detection, by object.
// D2-15668, D2-15672
pub type Sightings = BTreeMap<u64, Sighting>;

/// The settings of a detection: its context, where it stands (a place, or the detecting body with
/// a height, a scale and offsets), the target it centers on, what blocks its offset and its lines
/// of sight, its volume, the kinds it reports, and the objects it targets and ignores.
// D2-15655
#[derive(Debug, PartialEq)]
pub struct DetectorSettings {
    pub context: u32,
    pub base_position: Position,
    pub based_on_object: bool,
    pub height_adjustment: f32,
    pub use_object_scale: bool,
    pub base_offset_from_object: Vec3,
    pub offset_to_detection: Vec3,
    pub detect_at_blocked_offset: bool,
    pub follow_landscape: bool,
    pub center_on_target: u64,
    pub target_height_adjustment: f32,
    pub target_use_scale: bool,
    pub target_offset: Vec3,
    pub max_target_distance: f32,
    pub offset_physics_blocking: PhysicsTypes,
    pub offset_object_blocking: ObjectEthereality,
    /// Not sent.
    pub los_resolution: u32,
    pub volume: Volume,
    pub report: ReportType,
    pub targets: Vec<u64>,
    pub ignored: Vec<u64>,
    pub physics_blocking: PhysicsTypes,
    pub object_blocking: ObjectEthereality,
}

impl Default for DetectorSettings {
    // D2-15654
    fn default() -> Self {
        Self {
            context: 0,
            base_position: origin(),
            based_on_object: true,
            height_adjustment: 0.5,
            use_object_scale: false,
            base_offset_from_object: Vec3::ZERO,
            offset_to_detection: Vec3::ZERO,
            detect_at_blocked_offset: false,
            follow_landscape: false,
            center_on_target: 0,
            target_height_adjustment: 0.0,
            target_use_scale: false,
            target_offset: Vec3::ZERO,
            max_target_distance: 0.0,
            offset_physics_blocking: PhysicsTypes::default(),
            offset_object_blocking: ObjectEthereality::default(),
            los_resolution: 1,
            volume: Volume::default(),
            report: ReportType::default(),
            targets: Vec::new(),
            ignored: Vec::new(),
            physics_blocking: PhysicsTypes::default(),
            object_blocking: ObjectEthereality::default(),
        }
    }
}

impl Clone for DetectorSettings {
    // D2-15659, D2-04306
    fn clone(&self) -> Self {
        let mut volume = Volume::default();
        self.volume.copy_into(&mut volume);
        Self {
            base_position: self.base_position,
            offset_object_blocking: self.offset_object_blocking.clone(),
            volume,
            targets: self.targets.clone(),
            ignored: self.ignored.clone(),
            object_blocking: self.object_blocking.clone(),
            ..*self
        }
    }
}

/// `x - floor(x / 360) * 360` in double precision, rounded to a float at the end.
fn wrap_degrees(x: f32) -> f32 {
    let x = f64::from(x);
    let turns = (x * f64::from(0.002_777_777_8_f32)).floor();
    #[allow(clippy::cast_possible_truncation)]
    let w = (x - turns * 360.0) as f32;
    w
}

impl DetectorSettings {
    /// The box around the settings' volume ([`Volume::global_box`]).
    // D2-15647
    #[must_use]
    pub fn global_box(&self) -> Option<BBox> {
        self.volume.global_box()
    }

    pub fn set_shape_box(&mut self, y0: f32, y1: f32, x0: f32, x1: f32, z0: f32, z1: f32) {
        self.volume.create_box(y0, y1, x0, x1, z0, z1);
    }

    pub fn set_shape_sphere(&mut self, radius: f32, center: Vec3) {
        self.volume.create_sphere(radius, center);
    }

    pub fn set_shape_sphere_cone(&mut self, center: Vec3, radius: f32, dir: Vec3, angle: f32) {
        self.volume.create_sphere_cone(center, radius, dir, angle);
    }

    /// A shell between two radii (put in order, at least 0), between two headings (each brought
    /// into 0 to 360 degrees) and two elevations (put in order, each held to -90 to 90 degrees).
    #[allow(clippy::too_many_arguments)]
    pub fn set_shape_attack_cone(
        &mut self,
        center: Vec3,
        r0: f32,
        r1: f32,
        theta0: f32,
        theta1: f32,
        phi0: f32,
        phi1: f32,
    ) {
        // Retail's test is "not less or equal", so unordered values swap as well.
        let (mut lo, mut hi) = if r0 <= r1 { (r0, r1) } else { (r1, r0) };
        if lo < 0.0 {
            lo = 0.0;
        }
        if hi < 0.0 {
            hi = 0.0;
        }
        let t0 = wrap_degrees(theta0);
        let t1 = wrap_degrees(theta1);
        let (mut p_lo, mut p_hi) = if phi0 <= phi1 {
            (phi0, phi1)
        } else {
            (phi1, phi0)
        };
        if p_lo < -90.0 {
            p_lo = -90.0;
        }
        if p_hi < -90.0 {
            p_hi = -90.0;
        }
        if 90.0 < p_lo {
            p_lo = 90.0;
        }
        if 90.0 < p_hi {
            p_hi = 90.0;
        }
        self.volume
            .create_sphere_plane_cone(center, lo, hi, t0, t1, p_lo, p_hi);
    }

    /// An upright cylinder of `radius` around `center`, reaching `height` above it and `below`
    /// beneath it.
    pub fn set_shape_cylinder(&mut self, radius: f32, height: f32, below: f32, center: Vec3) {
        let low = Vec3::new(center.x, center.y, center.z - below);
        self.volume.create_cylinder(radius, low, height + below);
    }

    /// Stands on the detecting body, `height` up its height and moved by `offset`.
    pub fn use_detecting_object_for_base_position(
        &mut self,
        height: f32,
        use_scale: bool,
        offset: Vec3,
    ) {
        self.height_adjustment = height;
        self.based_on_object = true;
        self.use_object_scale = use_scale;
        self.offset_to_detection = offset;
    }

    /// Centers on a target: its id, how far it may be, its height and scale, the offset from it,
    /// what blocks the offset, and whether to detect where a blocked offset stops.
    #[allow(clippy::too_many_arguments)]
    pub fn set_detect_offset(
        &mut self,
        target: u64,
        max_distance: f32,
        height: f32,
        use_scale: bool,
        offset: Vec3,
        objects: &ObjectEthereality,
        physics: PhysicsTypes,
        at_blocked: bool,
    ) {
        self.max_target_distance = max_distance;
        self.offset_physics_blocking = physics;
        objects.copy_into(&mut self.offset_object_blocking);
        self.center_on_target = target;
        self.target_height_adjustment = height;
        self.target_use_scale = use_scale;
        self.target_offset = offset;
        self.detect_at_blocked_offset = at_blocked;
    }

    pub fn set_los_blocking_masks(
        &mut self,
        objects: &ObjectEthereality,
        physics: PhysicsTypes,
        resolution: u32,
    ) {
        self.physics_blocking = physics;
        objects.copy_into(&mut self.object_blocking);
        self.los_resolution = resolution;
    }

    /// Where the volume is.
    // D2-15646, D2-15650
    #[must_use]
    pub fn origin_point(&self) -> &Position {
        &self.volume.position
    }

    // D2-15658
    pub fn set_origin_point(&mut self, p: &Position) {
        self.volume.position = *p;
    }

    /// Stands at a place rather than on the detecting body.
    // D2-15649
    pub fn set_base_position(&mut self, p: &Position) {
        self.based_on_object = false;
        self.base_position = *p;
    }

    // D2-15651
    #[must_use]
    pub fn in_target_list(&self, object: u64) -> bool {
        self.targets.contains(&object)
    }

    // D2-15652
    #[must_use]
    pub fn in_exclude_list(&self, object: u64) -> bool {
        self.ignored.contains(&object)
    }

    /// Whether the detection targets exactly the one object it centers on.
    // D2-15653
    #[must_use]
    pub fn single_targeted_object(&self) -> bool {
        self.center_on_target != 0
            && self.targets.len() == 1
            && self.targets[0] == self.center_on_target
    }

    pub fn set_detection_object_type(&mut self, t: ReportType) {
        self.report = t;
    }

    pub fn add_target(&mut self, object: u64) {
        if !self.in_target_list(object) {
            self.targets.push(object);
        }
    }

    pub fn add_exclude_object(&mut self, object: u64) {
        if !self.in_exclude_list(object) {
            self.ignored.push(object);
        }
    }
}

impl Pack for DetectorSettings {
    // D2-15656
    fn pack(&self, w: &mut Writer) {
        w.u32(self.context);
        write_position(w, &self.base_position);
        w.bool32(self.based_on_object);
        w.f32(self.height_adjustment);
        w.bool32(self.use_object_scale);
        write_vec3(w, self.base_offset_from_object);
        write_vec3(w, self.offset_to_detection);
        w.bool32(self.detect_at_blocked_offset);
        w.bool32(self.follow_landscape);
        w.u64(self.center_on_target);
        w.f32(self.target_height_adjustment);
        w.bool32(self.target_use_scale);
        write_vec3(w, self.target_offset);
        w.f32(self.max_target_distance);
        self.offset_physics_blocking.pack(w);
        self.offset_object_blocking.pack(w);
        self.volume.pack(w);
        self.report.pack(w);
        write_ids(w, &self.targets);
        write_ids(w, &self.ignored);
        self.physics_blocking.pack(w);
        self.object_blocking.pack(w);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            context: r.u32()?,
            base_position: read_position(r)?,
            based_on_object: r.bool32()?,
            height_adjustment: r.f32()?,
            use_object_scale: r.bool32()?,
            base_offset_from_object: read_vec3(r)?,
            offset_to_detection: read_vec3(r)?,
            detect_at_blocked_offset: r.bool32()?,
            follow_landscape: r.bool32()?,
            center_on_target: r.u64()?,
            target_height_adjustment: r.f32()?,
            target_use_scale: r.bool32()?,
            target_offset: read_vec3(r)?,
            max_target_distance: r.f32()?,
            offset_physics_blocking: PhysicsTypes::unpack(r, ctx)?,
            offset_object_blocking: ObjectEthereality::unpack(r, ctx)?,
            los_resolution: 1,
            volume: Volume::unpack(r, ctx)?,
            report: ReportType::unpack(r, ctx)?,
            targets: read_ids(r)?,
            ignored: read_ids(r)?,
            physics_blocking: PhysicsTypes::unpack(r, ctx)?,
            object_blocking: ObjectEthereality::unpack(r, ctx)?,
        })
    }
}

/// The rays a line-of-sight check cast toward one object, and how much of it they found blocked.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectRays {
    pub object: u64,
    pub coverage: f32,
    pub rays: Vec<Ray>,
}

// D2-06868, D2-13368, D2-15628
impl ObjectRays {
    // D2-13367
    #[must_use]
    pub fn new(object: u64) -> Self {
        Self {
            object,
            ..Self::default()
        }
    }

    // D2-13370
    pub fn add_ray(&mut self, ray: Ray) {
        self.rays.push(ray);
    }
}

impl Pack for ObjectRays {
    // D2-13366, D2-06867
    fn pack(&self, w: &mut Writer) {
        w.u64(self.object);
        w.f32(self.coverage);
        write_packs(w, &self.rays);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            object: r.u64()?,
            coverage: r.f32()?,
            rays: read_packs(r, ctx)?,
        })
    }
}

/// The objects a line-of-sight check looks at in one landblock, with their rays.
// D2-15679, D2-06860
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockRays {
    pub block: u32,
    pub loaded: bool,
    pub objects: BTreeMap<u64, ObjectRays>,
}

impl BlockRays {
    // D2-15684
    #[must_use]
    pub fn new(block: u32) -> Self {
        Self {
            block,
            loaded: true,
            objects: BTreeMap::new(),
        }
    }

    /// Adds an object's rays; false when the object is already there.
    // D2-15681
    pub fn add_object(&mut self, rays: ObjectRays) -> bool {
        if self.objects.contains_key(&rays.object) {
            return false;
        }
        self.objects.insert(rays.object, rays);
        true
    }

    /// Adds a ray toward an object, adding the object when it is not there.
    // D2-15682
    pub fn add_ray_info(&mut self, ray: Ray, object: u64) {
        self.objects
            .entry(object)
            .or_insert_with(|| ObjectRays::new(object))
            .add_ray(ray);
    }
}

impl Pack for BlockRays {
    // D2-15673, D2-15683
    fn pack(&self, w: &mut Writer) {
        w.u32(self.block);
        w.bool32(self.loaded);
        write_table_header(w, self.objects.len());
        for (k, o) in &self.objects {
            w.u64(*k);
            o.pack(w);
        }
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let block = r.u32()?;
        let loaded = r.bool32()?;
        let objects = r
            .hash_table(12, |r| Ok((r.u64()?, ObjectRays::unpack(r, ctx)?)))?
            .into_iter()
            .collect();
        Ok(Self {
            block,
            loaded,
            objects,
        })
    }
}

/// The coverage at or above which a line of sight counts an object as hidden.
fn hidden_coverage() -> f64 {
    1.0 - f64::from(crate::geom::EPS)
}

/// A detection made at once: its context and settings, its sightings, the landblocks it looks in
/// and the rays it cast through each, how many of its blocks are elsewhere, and its line of sight.
// D2-15629
#[derive(Debug, Default, PartialEq)]
pub struct InstantDetection {
    pub context: u32,
    pub settings: DetectorSettings,
    /// Not sent.
    pub sightings: Sightings,
    pub blocks: Vec<BlockRays>,
    pub block_rays: Vec<BlockRays>,
    pub remote_block_count: u32,
    /// Not sent.
    pub los: SightLine,
}

impl Clone for InstantDetection {
    /// The sightings and the line of sight are not copied.
    // D2-15635, D2-06805
    fn clone(&self) -> Self {
        Self {
            context: self.context,
            settings: self.settings.clone(),
            sightings: Sightings::new(),
            blocks: self.blocks.clone(),
            block_rays: self.block_rays.clone(),
            remote_block_count: self.remote_block_count,
            los: SightLine::default(),
        }
    }
}

impl InstantDetection {
    // D2-15625
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts over with a context and settings.
    // D2-15626
    pub fn init(&mut self, context: u32, settings: &DetectorSettings) {
        self.sightings.clear();
        self.blocks.clear();
        self.block_rays.clear();
        self.remote_block_count = 0;
        self.context = context;
        self.settings = settings.clone();
    }

    // D2-15624
    pub fn block(&mut self, block: u32) -> Option<&mut BlockRays> {
        self.blocks.iter_mut().find(|b| b.block == block)
    }

    /// Forgets the sightings of the objects of a block that is no longer loaded.
    // D2-15630
    pub fn remove_objects_in_unloaded_block(&mut self, block: &BlockRays) {
        for k in block.objects.keys() {
            self.sightings.remove(k);
        }
    }

    /// Drops from a block the objects with no sighting.
    // D2-15631
    pub fn prune_objects(&self, block: &mut BlockRays) {
        block.objects.retain(|k, _| self.sightings.contains_key(k));
    }

    /// Takes a block's line-of-sight results: an object found hidden (coverage at least 0.9998)
    /// loses its sighting; otherwise its sighting keeps the greatest coverage found.
    // D2-15632
    pub fn update_profiles(&mut self, block: &BlockRays) {
        for (k, o) in &block.objects {
            let Some(s) = self.sightings.get_mut(k) else {
                continue;
            };
            // Retail compares the threshold in extended precision; unordered counts as visible.
            let hidden = hidden_coverage() <= f64::from(o.coverage);
            if hidden {
                self.sightings.remove(k);
            } else if s.los_coverage < o.coverage {
                s.los_coverage = o.coverage;
            }
        }
    }

    /// The block's entry in the looked-in blocks, made (loaded, empty) when absent.
    // D2-15636
    pub fn create_block_list_info(&mut self, block: u32) -> &mut BlockRays {
        let i = match self.blocks.iter().position(|b| b.block == block) {
            Some(i) => i,
            None => {
                self.blocks.push(BlockRays::new(block));
                self.blocks.len() - 1
            }
        };
        &mut self.blocks[i]
    }

    /// The block's entry in the cast rays, made (loaded, empty) when absent.
    // D2-15637
    pub fn find_create_block_ray(&mut self, block: u32) -> &mut BlockRays {
        let i = match self.block_rays.iter().position(|b| b.block == block) {
            Some(i) => i,
            None => {
                self.block_rays.push(BlockRays::new(block));
                self.block_rays.len() - 1
            }
        };
        &mut self.block_rays[i]
    }

    // D2-15638
    pub fn add_block_ray(&mut self, ray: Ray, object: u64, block: u32) {
        self.find_create_block_ray(block).add_ray_info(ray, object);
    }
}

impl Pack for InstantDetection {
    // D2-15623, D2-06858
    fn pack(&self, w: &mut Writer) {
        w.u32(self.context);
        self.settings.pack(w);
        w.u32(self.remote_block_count);
        write_packs(w, &self.blocks);
        write_packs(w, &self.block_rays);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            context: r.u32()?,
            settings: DetectorSettings::unpack(r, ctx)?,
            remote_block_count: r.u32()?,
            blocks: read_packs(r, ctx)?,
            block_rays: read_packs(r, ctx)?,
            sightings: Sightings::new(),
            los: SightLine::default(),
        })
    }
}
