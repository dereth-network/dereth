//! What an object of the world collides with: its shape, made from its look, and the cells it
//! reaches.
//!
//! An object's shape is the physics trees of its parts (every part of every part group, with the
//! meshes its look swaps in), each tested in the object's own frame, scaled by the object's scale;
//! failing trees, the upright cylinders of its setup; failing those, its setup's spheres. The
//! object stands in the cells the box around that shape reaches ([`crate::cells`]): the box its
//! setup gives around its physics when it has trees, else the box around its cylinders or spheres,
//! else just its place.

use std::sync::Arc;

use auberean_assets::mesh::Mesh;
use auberean_assets::property::PropertyCollection;
use auberean_primitives::DataId;
use auberean_world::look::{Group, Look};
use auberean_world::math::Mat4;
use auberean_world::place::{look_parts, PlaceRecords};
use auberean_world::records::WorldRecords;
use dereth_physics::geom::BspTree;
use dereth_physics::source::PhysicsPart;
use dereth_physics::V3;
use dereth_primitives::shape::{CylSphere, Sphere};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::cells;
use crate::geom::BBox;
use crate::land::{Land, Solid};
use crate::mesh::Trees;

/// The deepest chain of parent entities followed to an object's look.
const MAX_DEPTH: usize = 16;

fn v3(v: auberean_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// The line from `from` to `to` (in the landblock frame of `at`) taken into `at`'s own frame and
/// divided by `scale` (an axis of scale 0 left as it is).
fn unscaled_line(at: &Frame, scale: Vec3, from: Vec3, to: Vec3) -> (Vec3, Vec3) {
    let unscale = |v: Vec3| {
        let k = |a: f32, b: f32| if b == 0.0 { a } else { a / b };
        Vec3::new(k(v.x, scale.x), k(v.y, scale.y), k(v.z, scale.z))
    };
    (
        unscale(crate::geom::point_to_local(at, from)),
        unscale(crate::geom::point_to_local(at, to)),
    )
}

/// An object's collision shape, in its own frame.
#[derive(Clone, Debug)]
pub struct Shape {
    /// The physics trees of its parts.
    pub trees: Vec<Arc<BspTree>>,
    /// Its setup's upright cylinders and spheres.
    pub cylinders: Vec<CylSphere>,
    pub spheres: Vec<Sphere>,
    /// The box its setup gives around its physics, unscaled.
    pub physics_box: BBox,
    /// The object's scale: its entities' and its look's.
    pub scale: Vec3,
    /// The radius its setup gives around its physics, unscaled.
    pub containment_radius: f32,
    /// The box around what its parts draw, unscaled: empty (a point at its place) when it draws
    /// nothing.
    pub render_box: BBox,
    /// The property collections of the entities it is made from, innermost first, as an object
    /// made from them takes them; none for an object made straight from a look or a setup.
    pub properties: Vec<Arc<PropertyCollection>>,
}

impl Shape {
    /// The shape of object `id`: a look, a setup, or an entity made from one of those (followed
    /// through its parents). None when nothing of it can be found.
    #[must_use]
    pub fn of(records: &WorldRecords, id: DataId, trees: &mut Trees) -> Option<Self> {
        let mut scale = Vec3::new(1.0, 1.0, 1.0);
        let mut at = id;
        let mut properties = Vec::new();
        for _ in 0..MAX_DEPTH {
            if at.0 >> 24 != 0x47 {
                break;
            }
            let e = records.entity(at)?;
            if let Some(s) = e.scale {
                scale = Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z);
            }
            properties.push(Arc::new(e.properties.clone().unwrap_or_default()));
            at = DataId(e.object?);
        }
        properties.reverse();
        let mut look = match at.0 >> 24 {
            0x1F => Look::of_record(records, at).ok()?,
            0x04 => {
                let mut l = Look::default();
                l.add_group(&Group {
                    key: 1,
                    setup: at,
                    ..Group::default()
                })
                .ok()?;
                l
            }
            _ => return None,
        };
        look.scale = auberean_primitives::Vec3::new(
            scale.x * look.scale.x,
            scale.y * look.scale.y,
            scale.z * look.scale.z,
        );
        let mut shape = Self::of_look(records, &look, trees)?;
        shape.properties = properties;
        Some(shape)
    }

    /// The shape of an object drawn with `look`: the trees of its parts' meshes, its root
    /// setup's cylinders, spheres and physics box, and the look's scale.
    #[must_use]
    pub fn of_look(records: &WorldRecords, look: &Look, trees: &mut Trees) -> Option<Self> {
        let root = look.groups.get(look.root?)?.setup;
        let setup = records.setup(root);
        let one = auberean_primitives::Vec3::new(1.0, 1.0, 1.0);
        let parts = look_parts(records as &dyn PlaceRecords, look, &Mat4::IDENTITY, one);
        let part_trees = parts
            .iter()
            .filter_map(|p| trees.get(p.mesh.0, |m| records.get::<Mesh>(m)))
            .collect();
        let containment_radius = setup.as_ref().map_or(0.0, |s| s.physics_radius);
        let (cylinders, spheres, physics_box) = setup.map_or_else(Default::default, |s| {
            (
                s.cylinders
                    .iter()
                    .map(|c| CylSphere {
                        low_pt: v3(c.low),
                        radius: c.radius,
                        height: c.height,
                    })
                    .collect(),
                s.spheres
                    .iter()
                    .map(|b| Sphere::new(v3(b.center), b.radius))
                    .collect(),
                BBox::new(v3(s.physics_bounds.min), v3(s.physics_bounds.max)),
            )
        });
        Some(Self {
            trees: part_trees,
            cylinders,
            spheres,
            physics_box,
            scale: v3(look.scale),
            containment_radius,
            render_box: render_box(records, look, trees),
            properties: Vec::new(),
        })
    }

    /// The shape of an object put together as `parts` and standing at `scale`: the physics trees
    /// of every part of every group, and its root setup's cylinders, spheres, physics box and
    /// physics radius.
    #[must_use]
    pub fn of_parts(parts: &crate::parts::PartGroup, scale: Vec3) -> Self {
        let mut trees = Vec::new();
        let mut stack = vec![parts];
        while let Some(g) = stack.pop() {
            trees.extend(
                g.array
                    .parts
                    .iter()
                    .flatten()
                    .filter_map(|p| p.mesh.as_ref().and_then(|m| m.tree.clone())),
            );
            stack.extend(g.children.iter().rev());
        }
        let a = &parts.array;
        let mut render_box = <BBox as crate::geom::BoxOps>::for_adjustment();
        parts.grow_render_box(&mut render_box);
        Self {
            trees,
            cylinders: a
                .cylinders()
                .iter()
                .map(|c| CylSphere {
                    low_pt: v3(c.low),
                    radius: c.radius,
                    height: c.height,
                })
                .collect(),
            spheres: a
                .spheres()
                .iter()
                .map(|b| Sphere::new(v3(b.center), b.radius))
                .collect(),
            physics_box: a.physics_bounding_box(),
            scale,
            containment_radius: a.physics_containment_radius(),
            render_box: if render_box.min.x <= render_box.max.x {
                render_box
            } else {
                BBox::default()
            },
            properties: Vec::new(),
        }
    }

    /// Whether any part has a physics tree.
    // D2-06277, D2-06468, D2-06581, D2-06629, D2-06682
    #[must_use]
    pub fn has_physics_tree(&self) -> bool {
        !self.trees.is_empty()
    }

    /// The box around its physics, scaled.
    // D2-06122, D2-06466, D2-06622
    #[must_use]
    pub fn physics_bounding_box(&self) -> BBox {
        let (b, s) = (self.physics_box, self.scale);
        BBox::new(
            Vec3::new(b.min.x * s.x, b.min.y * s.y, s.z * b.min.z),
            Vec3::new(b.max.x * s.x, b.max.y * s.y, s.z * b.max.z),
        )
    }

    /// Where the line from `from` to `to` (both in the landblock frame of `at`) first meets this
    /// shape standing at `at`, as a fraction of the way along it: the line is taken into the
    /// shape's own frame and divided by its scale, then met with its spheres when it has any, else
    /// with its upright cylinders, else with the polygons of its physics trees. A sphere or a
    /// cylinder met only at the far end or beyond does not count. None when nothing is met.
    #[must_use]
    pub fn check_ray(&self, at: &dereth_primitives::Frame, from: Vec3, to: Vec3) -> Option<f32> {
        self.check_ray_facing(at, from, to).map(|(t, _)| t)
    }

    /// [`Self::check_ray`], with what the line meets facing it: a sphere or a cylinder straight back
    /// along the line, a tree by its polygon's facing, turned into the world.
    // D2-06184, D2-06278
    #[must_use]
    pub fn check_ray_facing(
        &self,
        at: &dereth_primitives::Frame,
        from: Vec3,
        to: Vec3,
    ) -> Option<(f32, Vec3)> {
        use crate::geom::{CylinderOps, Ray, SphereOps};
        if self.spheres.is_empty() && self.cylinders.is_empty() {
            return self.check_trees_facing(at, self.scale, from, to);
        }
        let (a, b) = unscaled_line(at, self.scale, from, to);
        let ray = Ray::segment(a, b);
        let mut best = 1.0_f32;
        let mut hit = false;
        let mut take = |t: Option<f32>| {
            if let Some(t) = t.filter(|t| *t < best) {
                best = t;
                hit = true;
            }
        };
        if self.spheres.is_empty() {
            for c in &self.cylinders {
                take(CylinderOps::trace_ray(c, &ray));
            }
        } else {
            for sp in &self.spheres {
                take(SphereOps::trace_ray(sp, &ray));
            }
        }
        let line = Ray::segment(from, to);
        let back = Vec3::new(-line.dir.x, -line.dir.y, -line.dir.z);
        hit.then_some((best, back))
    }

    /// Where the line from `from` to `to` (both in the landblock frame of `at`) first meets the
    /// polygons of this shape's physics trees standing at `at` at `scale`, as a fraction of the
    /// way along it; none when it meets none.
    #[must_use]
    pub fn check_trees(
        &self,
        at: &dereth_primitives::Frame,
        scale: Vec3,
        from: Vec3,
        to: Vec3,
    ) -> Option<f32> {
        self.check_trees_facing(at, scale, from, to).map(|(t, _)| t)
    }

    /// [`Self::check_trees`], with the facing of the polygon met, turned into the world.
    #[must_use]
    pub fn check_trees_facing(
        &self,
        at: &dereth_primitives::Frame,
        scale: Vec3,
        from: Vec3,
        to: Vec3,
    ) -> Option<(f32, Vec3)> {
        let (a, b) = unscaled_line(at, scale, from, to);
        let mut best: Option<(f32, Vec3)> = None;
        for tree in &self.trees {
            for poly in &tree.polygons {
                if let Some(t) = crate::land::segment_polygon(a, b, &poly.vertices) {
                    if best.is_none_or(|(x, _)| t < x) {
                        best = Some((t, poly.plane.normal));
                    }
                }
            }
        }
        best.map(|(t, n)| {
            let turn = dereth_physics::math::l2g(at.rotation);
            (t, dereth_physics::math::localtoglobalvec(turn, n))
        })
    }

    /// The box around what it draws, scaled.
    // D2-06119
    #[must_use]
    pub fn render_bounding_box(&self) -> BBox {
        let (b, s) = (self.render_box, self.scale);
        BBox::new(
            Vec3::new(b.min.x * s.x, b.min.y * s.y, b.min.z * s.z),
            Vec3::new(b.max.x * s.x, b.max.y * s.y, b.max.z * s.z),
        )
    }

    /// The cells a static object placed by its cell or landblock stands in at `pos`: those the box
    /// around what it draws touches, whatever it collides with.
    // D2-06254
    #[must_use]
    pub fn static_cells(&self, land: &Land, pos: &Position) -> Vec<CellId> {
        cells::find_cells_of_box(land, pos, &self.render_bounding_box())
    }

    /// The cells the object reaches standing at `pos`: those the box around its trees touches,
    /// else the box around its cylinders, else around its spheres, else its place alone.
    // D2-06255
    #[must_use]
    pub fn cross_cells(&self, land: &Land, pos: &Position) -> Vec<CellId> {
        if self.has_physics_tree() {
            cells::find_cells_of_box(land, pos, &self.physics_bounding_box())
        } else if !self.cylinders.is_empty() {
            cells::find_cells_of_cylinders(land, pos, &self.cylinders)
        } else if !self.spheres.is_empty() {
            cells::find_cells_of_spheres(land, pos, &self.spheres)
        } else {
            cells::find_cells_of_box(land, pos, &BBox::default())
        }
    }
}

/// A placed entity of the world's records that collides only through its setup's cylinders or
/// spheres (one with physics trees is among its block's solids instead): where it stands, in
/// `cell`, and its shape with the entity's own scale. None for an entity that is not drawn or has
/// nothing to collide with.
#[must_use]
pub fn placed_entity(
    records: &WorldRecords,
    trees: &mut Trees,
    e: &auberean_assets::entity::Entity,
    cell: CellId,
) -> Option<(Position, Shape)> {
    if !e
        .kind
        .is_some_and(|k| auberean_world::place::DRAWN_KINDS.contains(&k))
    {
        return None;
    }
    let mut shape = Shape::of(records, DataId(e.object?), trees)?;
    shape.properties.push(own_properties(e));
    let by_tree = shape.has_physics_tree();
    if (by_tree && !marks_ethereal(&shape.properties))
        || (!by_tree && shape.cylinders.is_empty() && shape.spheres.is_empty())
    {
        return None;
    }
    shape.scale = entity_scale(e, shape.scale);
    Some((Position::new(cell, entity_frame(e)), shape))
}

/// Whether any of these collections sets a bit of an object's physics type: the object is then
/// something some mover passes through.
fn marks_ethereal(collections: &[Arc<PropertyCollection>]) -> bool {
    collections
        .iter()
        .flat_map(|c| c.groups.iter())
        .filter(|g| g.group == crate::world::group::ETHEREAL)
        .flat_map(|g| g.properties.iter())
        .any(|p| {
            matches!(
                p.value,
                auberean_assets::property::PropertyValue::Bool(true)
            )
        })
}

/// The solid parts of a placed entity of the world's records: one for each part of its look whose
/// mesh has a physics tree (every part of every part group, with the meshes its look swaps in),
/// every one standing where the entity stands, in `cell`, at the object's scale (the entity's
/// times its shape's). A placed entity is an object like any other, and an object's parts are
/// tested in the object's own frame: the bone a part is drawn on, or the connection point its
/// group hangs from, does not move where it collides, even where that is meters from where it is
/// drawn. Each is tested at the object's scale along each axis. Empty for an entity that is not
/// drawn or whose parts have no trees.
#[must_use]
pub fn placed_solids(
    records: &WorldRecords,
    trees: &mut Trees,
    e: &auberean_assets::entity::Entity,
    cell: CellId,
) -> Vec<Solid> {
    if !e
        .kind
        .is_some_and(|k| auberean_world::place::DRAWN_KINDS.contains(&k))
    {
        return Vec::new();
    }
    let Some(shape) = e.object.and_then(|o| Shape::of(records, DataId(o), trees)) else {
        return Vec::new();
    };
    let scale = entity_scale(e, shape.scale);
    let frame = entity_frame(e);
    let render_box = Shape {
        scale,
        ..shape.clone()
    }
    .render_bounding_box();
    let mut properties = shape.properties.clone();
    properties.push(own_properties(e));
    let marked = marks_ethereal(&properties);
    shape
        .trees
        .iter()
        .filter_map(|tree| {
            let root = tree.root()?;
            let center =
                dereth_physics::math::localtoglobal(&frame, root.sphere.center.mul(scale.z));
            Some(Solid {
                scale,
                part: PhysicsPart {
                    pos: Position::new(cell, frame),
                    gfxobj_scale: scale.z,
                    physics_bsp: Some(Arc::clone(tree)),
                    ..PhysicsPart::default()
                },
                bounds: Sphere::new(
                    center,
                    root.sphere.radius * scale.x.max(scale.y).max(scale.z),
                ),
                render_box,
                properties: properties.clone(),
                marked,
            })
        })
        .collect()
}

/// The box around what an object drawn with `look` draws: its part groups put together, every
/// group's parts' boxes (each carried through its part's transform only when its setup has no
/// bones); a point at its place when it draws nothing.
fn render_box(records: &WorldRecords, look: &Look, trees: &mut Trees) -> BBox {
    let mut group = crate::parts::PartGroup::default();
    group.init_look(&crate::parts::CachedParts::new(records, trees), look);
    group.cache_render_box();
    let b = group.array.render_bounds;
    if b.min.x <= b.max.x && b.min.y <= b.max.y && b.min.z <= b.max.z {
        b
    } else {
        BBox::default()
    }
}

/// A placed entity's own property collection (empty when it has none).
fn own_properties(e: &auberean_assets::entity::Entity) -> Arc<PropertyCollection> {
    Arc::new(e.properties.clone().unwrap_or_default())
}

/// `scale` times a placed entity's own scale.
fn entity_scale(e: &auberean_assets::entity::Entity, scale: Vec3) -> Vec3 {
    e.scale.map_or(scale, |s| {
        Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z)
    })
}

/// Where a placed entity stands, in its landblock's frame.
fn entity_frame(e: &auberean_assets::entity::Entity) -> Frame {
    e.offset
        .as_ref()
        .map_or(Frame::new(Vec3::ZERO, Quat::IDENTITY), |f| {
            let (o, q) = (f.frame.origin, f.frame.rotation);
            Frame::new(v3(o), Quat::new(q.w, q.x, q.y, q.z))
        })
}
