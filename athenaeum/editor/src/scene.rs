//! The world as the client draws it, seen from the free camera: the landscape, its scenery and
//! placed objects and the sky around the camera (rebuilt when the camera enters another cell),
//! and the world's rows standing in it as bodies, each put together from its template's look as
//! the client puts together what the server sends; and the ground under a point.

use std::collections::{HashMap, HashSet};

use auberean_animation::body::{Body as Figure, PartPose};
use auberean_animation::look::{Desc, Keyed, Look, GLOBAL_KEY};
use auberean_animation::CachedRecords;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_draw::body::BodyDrawer;
use auberean_draw::effects::ObjectEffects;
use auberean_draw::scene::{inside_light, Camera, Frame, ObjectBatch, SceneBuilder, Spot};
use auberean_draw::systems::ParticleSystems;
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::color::Color;
use auberean_world::math::{self, Mat4};
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::sky::DayLight;
use dereth_primitives::num::math as fm;
use dereth_primitives::{CellId, LandblockId};

use crate::view::{View, BLOCK};

/// The world's records and its region, read once.
pub struct WorldData {
    pub records: WorldRecords,
    pub region: RegionSet,
}

impl std::fmt::Debug for WorldData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldData").finish_non_exhaustive()
    }
}

impl WorldData {
    /// The world of the install in `dir`.
    ///
    /// # Errors
    /// The install lacks a file the world is drawn from, or its region does not read.
    pub fn open(dir: &std::path::Path) -> Result<Self, String> {
        let found = install_dats(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
            let (_, p) = found.iter().find(|(n, _)| want(n))?;
            DatFile::open(p).ok()
        };
        let portal = open(&|n| *n == DatName::Portal).ok_or("no portal file")?;
        let cell = open(&|n| *n == DatName::Cell(1)).ok_or("no cell file")?;
        let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
        let records = WorldRecords::new(portal, cell, open(&|n| *n == DatName::HighRes), ctx);
        if let Some(file) = open(&|n| *n == DatName::Local("English".to_owned())) {
            records.use_language_file(file);
        }
        let region = records.region().map_err(|e| format!("{e:?}"))?;
        Ok(Self { records, region })
    }
}

/// A row to stand in the world: its look, where on the world's grid, its rotation (scalar
/// first) and its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Standing {
    pub look: u32,
    pub at: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: f32,
}

/// A look put together once: its body at rest and its parts' poses, and the look itself where it
/// carries effects.
struct Posed {
    figure: Figure,
    poses: Vec<PartPose>,
    scale: Vec3,
    with_effects: Option<Look>,
}

/// One row's effects as they play, and the block they stand in.
struct RowEffects {
    effects: ObjectEffects,
    home: (i32, i32),
}

/// The world's frame as last built: for the camera's block, cell and moment of the day, with how
/// many of its objects are the world's own (the rows' bodies come after them).
type Built = ((i32, i32), (i32, i32), u32, Frame, usize);

/// What a look is made of: the corners of the box it fills, and its triangles.
#[derive(Debug)]
struct LookShape {
    lo: [f32; 3],
    hi: [f32; 3],
    triangles: Vec<[[f32; 3]; 3]>,
}

/// How many buckets a worn appearance's tables are kept in.
const WORN_BUCKETS: u32 = 4;

/// The description of `look` alone.
fn plain_desc(look: u32) -> Desc {
    Desc {
        parent: DataId(look),
        scale: Vec3::new(1.0, 1.0, 1.0),
        child_scale: 1.0,
        particle_scale: 1.0,
        global: auberean_animation::look::Group {
            key: GLOBAL_KEY,
            ..Default::default()
        },
        ..Desc::default()
    }
}

/// A picture's field of view, degrees. Narrow, so a tall thing keeps its shape.
const PICTURE_FOV: f32 = 30.0;
/// Where a picture is seen from, degrees: to the north-west of the thing, so what faces north
/// shows its front, and a little above.
const PICTURE_HEADING: f32 = 145.0;
const PICTURE_PITCH: f32 = -15.0;

/// A camera that holds all of the box from `lo` to `hi` in a square picture.
#[must_use]
pub fn picture_camera(lo: [f32; 3], hi: [f32; 3]) -> Camera {
    let center = Vec3::new(
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    );
    let half = [hi[0] - center.x, hi[1] - center.y, hi[2] - center.z];
    // Seen from anywhere, nothing in the box is further from its center than its corners are.
    let reach = (half[0] * half[0] + half[1] * half[1] + half[2] * half[2])
        .sqrt()
        .max(0.05);
    let distance = reach / fm::sinf((PICTURE_FOV / 2.0).to_radians());
    let (heading, pitch) = (PICTURE_HEADING.to_radians(), PICTURE_PITCH.to_radians());
    let along = Vec3::new(
        fm::sinf(heading) * fm::cosf(pitch),
        fm::cosf(heading) * fm::cosf(pitch),
        fm::sinf(pitch),
    );
    Camera {
        position: Vec3::new(
            center.x - along.x * distance,
            center.y - along.y * distance,
            center.z - along.z * distance,
        ),
        heading: PICTURE_HEADING,
        pitch: PICTURE_PITCH,
        fov_y: PICTURE_FOV,
        near: (distance - reach * 1.1).max(0.01),
        far: distance + reach * 1.1,
        orthographic: None,
    }
}

/// The world a window shows, from the free camera.
pub struct Scene<'w> {
    data: &'w WorldData,
    builder: SceneBuilder<'w>,
    anims: CachedRecords<'w>,
    drawer: BodyDrawer,
    land: Land,
    trees: Trees,
    loaded: HashSet<(u8, u8)>,
    built: Option<Built>,
    looks: HashMap<u32, Option<Posed>>,
    /// The vertical field of view, degrees.
    pub fov: f32,
    /// The moment of the day, 0 to 1 (midnight to midnight).
    pub time_of_day: f32,
    /// The blocks whose interior cells are read.
    cells_read: HashSet<u16>,
    /// What each look is made of, for clicks on it and for standing on it.
    shapes: HashMap<u32, Option<std::sync::Arc<LookShape>>>,
    /// The particle systems of what the rows play, and the rows' effects, by look and which of
    /// that look's rows they are.
    systems: ParticleSystems,
    row_effects: HashMap<(u32, usize), RowEffects>,
    started: std::time::Instant,
}

impl std::fmt::Debug for Scene<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene")
            .field("looks", &self.looks.len())
            .finish_non_exhaustive()
    }
}

impl<'w> Scene<'w> {
    #[must_use]
    pub fn new(data: &'w WorldData) -> Self {
        let mut builder = SceneBuilder::new(&data.records, &data.region);
        // The editor's frame is rebuilt as the camera flies: no shadows to work out each time.
        builder.static_shadows = false;
        builder.cloud_shadows = false;
        Self {
            data,
            builder,
            anims: CachedRecords::new(&data.records),
            drawer: BodyDrawer::default(),
            land: Land::new(),
            trees: Trees::default(),
            loaded: HashSet::new(),
            built: None,
            looks: HashMap::new(),
            fov: 70.0,
            time_of_day: 0.45,
            cells_read: HashSet::new(),
            shapes: HashMap::new(),
            systems: ParticleSystems::default(),
            row_effects: HashMap::new(),
            started: std::time::Instant::now(),
        }
    }

    /// A block's ground and what stands on it, read when first asked for.
    fn read_block(&mut self, bx: u8, by: u8) {
        if self.loaded.insert((bx, by)) {
            self.land.add_block(
                &self.data.records,
                &self.data.region,
                &mut self.trees,
                bx,
                by,
            );
        }
    }

    /// Where a look from `from` along `dir` (on the world's grid, within `reach` meters) first meets
    /// what the world builds outdoors, its stones, platforms and buildings: how far along, and
    /// whether that much of it is level enough to stand on.
    pub fn built_met(&mut self, from: [f32; 3], dir: [f32; 3], reach: f32) -> Option<(f32, bool)> {
        let block_of = |v: f32| (v / BLOCK).floor();
        let (cx, cy) = (block_of(from[0]), block_of(from[1]));
        let span = (reach / BLOCK).ceil();
        let mut best: Option<(f32, bool)> = None;
        let mut take = |t: f32, up: f32| {
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, up > 0.5));
            }
        };
        let v = |p: [f32; 3]| dereth_primitives::Vec3::new(p[0], p[1], p[2]);
        let mut by = cy - span;
        while by <= cy + span {
            let mut bx = cx - span;
            while bx <= cx + span {
                if (0.0..=255.0).contains(&bx) && (0.0..=255.0).contains(&by) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let (x, y) = (bx as u8, by as u8);
                    self.read_block(x, y);
                    if let Some(block) = self.land.block(x, y) {
                        let start = [from[0] - bx * BLOCK, from[1] - by * BLOCK, from[2]];
                        let end = [
                            start[0] + dir[0] * reach,
                            start[1] + dir[1] * reach,
                            start[2] + dir[2] * reach,
                        ];
                        for solid in &block.solids {
                            let Some(tree) = solid.part.physics_bsp.as_ref() else {
                                continue;
                            };
                            let frame = &solid.part.pos.frame;
                            let k = |a: f32, b: f32| if b == 0.0 { a } else { a / b };
                            let inward = |p: [f32; 3]| {
                                let l = dereth_physics::math::globaltolocal(frame, v(p));
                                dereth_primitives::Vec3::new(
                                    k(l.x, solid.scale.x),
                                    k(l.y, solid.scale.y),
                                    k(l.z, solid.scale.z),
                                )
                            };
                            let (a, b) = (inward(start), inward(end));
                            let turn = dereth_physics::math::l2g(frame.rotation);
                            for poly in &tree.polygons {
                                let corners: Vec<[f32; 3]> =
                                    poly.vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
                                let along = [b.x - a.x, b.y - a.y, b.z - a.z];
                                let hit =
                                    crate::view::ray_polygon([a.x, a.y, a.z], along, &corners);
                                if let Some(t) = hit.filter(|t| *t <= 1.0) {
                                    let n = dereth_physics::math::localtoglobalvec(
                                        turn,
                                        poly.plane.normal,
                                    );
                                    take(t * reach, n.z.abs());
                                }
                            }
                        }
                        for (pos, shape) in &block.objects {
                            if let Some((t, n)) =
                                shape.check_ray_facing(&pos.frame, v(start), v(end))
                            {
                                take(t * reach, n.z.abs());
                            }
                        }
                    }
                }
                bx += 1.0;
            }
            by += 1.0;
        }
        best
    }

    /// The ground's height under a point of the world's grid, `None` off the world.
    pub fn ground(&mut self, x: f32, y: f32) -> Option<f32> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (bx, by) = (
            (x / BLOCK).floor().clamp(0.0, 255.0) as u8,
            (y / BLOCK).floor().clamp(0.0, 255.0) as u8,
        );
        self.read_block(bx, by);
        let local =
            dereth_primitives::Vec3::new(x - f32::from(bx) * BLOCK, y - f32::from(by) * BLOCK, 0.0);
        self.land
            .ground(LandblockId::new(bx, by), local)
            .map(|(z, _)| z)
    }

    /// The interior cells of a block, read when first asked for.
    fn read_cells(&mut self, lb: u16) {
        if self.cells_read.insert(lb) {
            let [x, y] = lb.to_be_bytes();
            add_block_cells(&self.land, &self.data.records, &mut self.trees, x, y);
        }
    }

    /// The interior cell of block `lb` a point of that block's frame lies in, named in full.
    pub fn cell_holding(&mut self, lb: u16, at: [f32; 3]) -> Option<u32> {
        self.read_cells(lb);
        let point = dereth_primitives::Vec3::new(at[0], at[1], at[2]);
        self.land
            .interior_cells_of(u32::from(lb) << 16)
            .into_iter()
            .find(|id| {
                self.land.cell(*id).is_some_and(|g| {
                    let local = dereth_physics::math::globaltolocal(&g.frame, point);
                    g.cell_bsp
                        .as_ref()
                        .is_some_and(|t| t.point_inside_cell_bsp(local))
                })
            })
            .map(|id| id.0)
    }

    /// Where a look from `from` along `dir` (of block `lb`'s frame) first meets a floor of a room
    /// seen from cell `low`, and that room's id. A look that meets a wall first gets the floor at
    /// the wall's foot; a look that meets nothing, nothing.
    pub fn floor_seen(
        &mut self,
        lb: u16,
        low: u16,
        from: [f32; 3],
        dir: [f32; 3],
    ) -> Option<(u32, [f32; 3])> {
        let (cell, at, floor) = self.first_met(lb, low, from, dir)?;
        if floor {
            return Some((cell, at));
        }
        // A little back toward the eye, off the wall, and straight down.
        let off = [
            at[0] - dir[0] * 0.05,
            at[1] - dir[1] * 0.05,
            at[2] - dir[2] * 0.05,
        ];
        let (cell, at, floor) = self.first_met(lb, low, off, [0.0, 0.0, -1.0])?;
        floor.then_some((cell, at))
    }

    /// The room and point a look first meets, and whether that is level enough to stand on.
    fn first_met(
        &mut self,
        lb: u16,
        low: u16,
        from: [f32; 3],
        dir: [f32; 3],
    ) -> Option<(u32, [f32; 3], bool)> {
        self.read_cells(lb);
        let home = (u32::from(lb) << 16) | u32::from(low);
        let here = self.land.cell(CellId(home))?;
        let mut best: Option<(f32, u32, bool)> = None;
        for id in std::iter::once(CellId(home)).chain(here.stab_list.iter().copied()) {
            let Some(g) = self.land.cell(id) else {
                continue;
            };
            let v = |p: [f32; 3]| dereth_primitives::Vec3::new(p[0], p[1], p[2]);
            let start = dereth_physics::math::globaltolocal(&g.frame, v(from));
            let end = dereth_physics::math::globaltolocal(
                &g.frame,
                v([from[0] + dir[0], from[1] + dir[1], from[2] + dir[2]]),
            );
            let along = [end.x - start.x, end.y - start.y, end.z - start.z];
            for polygon in &g.physics_polygons {
                let corners: Vec<[f32; 3]> =
                    polygon.vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
                let begin = [start.x, start.y, start.z];
                if let Some(t) = crate::view::ray_polygon(begin, along, &corners) {
                    if best.is_none_or(|b| t < b.0) {
                        // Level enough to stand on, whichever way round the polygon was wound.
                        best = Some((t, id.0, polygon.plane.normal.z.abs() > 0.5));
                    }
                }
            }
        }
        let (t, cell, floor) = best?;
        Some((
            cell,
            [
                from[0] + dir[0] * t,
                from[1] + dir[1] * t,
                from[2] + dir[2] * t,
            ],
            floor,
        ))
    }

    fn posed(&mut self, look: u32) -> bool {
        if !self.looks.contains_key(&look) {
            let desc = plain_desc(look);
            let posed = Look::of(&self.anims, &desc).ok().and_then(|l| {
                let figure = Figure::from_look(&self.anims, &l)?;
                let poses = figure.pose(&self.anims);
                Some(Posed {
                    figure,
                    poses,
                    scale: l.scale,
                    with_effects: l.global.has_fx().then(|| l.clone()),
                })
            });
            self.looks.insert(look, posed);
        }
        self.looks.get(&look).is_some_and(Option::is_some)
    }

    /// What a look is made of when it stands at the origin unturned, at its own scale: the box it
    /// fills, and its triangles.
    fn look_shape(&mut self, look: u32) -> Option<std::sync::Arc<LookShape>> {
        if let Some(known) = self.shapes.get(&look) {
            return known.clone();
        }
        let found = if self.posed(look) {
            let p = self.looks.get(&look).and_then(Option::as_ref);
            p.and_then(|p| {
                let place = Mat4::from_parts(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, p.scale);
                let mut shape = LookShape {
                    lo: [f32::MAX; 3],
                    hi: [f32::MIN; 3],
                    triangles: Vec::new(),
                };
                let records = &self.data.records;
                for b in self.drawer.batches(records, &p.figure, &p.poses, &place) {
                    let at = |i: u16| -> Option<[f32; 3]> {
                        let v = b.vertices.get(usize::from(i))?;
                        let q = b.transform.transform4([
                            v.position[0],
                            v.position[1],
                            v.position[2],
                            1.0,
                        ]);
                        Some([q[0], q[1], q[2]])
                    };
                    for t in b.indices.as_chunks::<3>().0 {
                        if let (Some(a), Some(c), Some(d)) = (at(t[0]), at(t[1]), at(t[2])) {
                            for q in [a, c, d] {
                                for ((lo, hi), v) in shape.lo.iter_mut().zip(&mut shape.hi).zip(q) {
                                    *lo = lo.min(v);
                                    *hi = hi.max(v);
                                }
                            }
                            shape.triangles.push([a, c, d]);
                        }
                    }
                }
                (shape.lo[0] <= shape.hi[0]).then(|| std::sync::Arc::new(shape))
            })
        } else {
            None
        };
        self.shapes.insert(look, found.clone());
        found
    }

    /// Where a line from `from` along `along`, in a look's own space, first meets what it draws: how
    /// far along, and whether that face is level enough to stand on.
    pub fn look_met(&mut self, look: u32, from: [f32; 3], along: [f32; 3]) -> Option<(f32, bool)> {
        let shape = self.look_shape(look)?;
        crate::view::ray_box(from, along, shape.lo, shape.hi)?;
        let mut best: Option<(f32, bool)> = None;
        for t in &shape.triangles {
            if let Some(d) = crate::view::ray_polygon(from, along, t) {
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, crate::view::is_level(t)));
                }
            }
        }
        best
    }

    /// The overview map's picture.
    #[must_use]
    pub fn map_picture(&self) -> Vec<u8> {
        crate::map::picture(self.data)
    }

    /// A point in the middle of a room, in its landblock, by what its walls and floors reach.
    pub fn room_point(&mut self, cell: u32) -> Option<[f32; 3]> {
        #[allow(clippy::cast_possible_truncation)]
        self.read_cells((cell >> 16) as u16);
        let c = self.land.cell(CellId(cell))?;
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in c.physics_polygons.iter().flat_map(|p| &p.vertices) {
            let w = dereth_physics::math::localtoglobal(&c.frame, *p);
            for ((lo, hi), v) in lo.iter_mut().zip(&mut hi).zip([w.x, w.y, w.z]) {
                *lo = lo.min(v);
                *hi = hi.max(v);
            }
        }
        (lo[0] <= hi[0]).then(|| {
            [
                (lo[0] + hi[0]) / 2.0,
                (lo[1] + hi[1]) / 2.0,
                (lo[2] + 1.8).min(hi[2]),
            ]
        })
    }

    /// Whether a look is a body of several parts that moves: a person, not a barrel or a portal.
    pub fn is_moving_body(&mut self, look: u32) -> bool {
        Look::of(&self.anims, &plain_desc(look))
            .is_ok_and(|l| l.motion_desc != DataId::NONE && l.groups.entries.len() > 1)
    }

    /// A picture of `look` alone: its parts and body at rest in a plain light, framed to the box
    /// it fills. `None` for a look that draws nothing.
    pub fn picture_frame(&mut self, look: u32) -> Option<Frame> {
        let shape = self.look_shape(look)?;
        let p = self.looks.get(&look)?.as_ref()?;
        let place = Mat4::from_parts(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, p.scale);
        let bodies = self
            .drawer
            .batches(&self.data.records, &p.figure, &p.poses, &place);
        Some(self.picture_of(bodies, picture_camera(shape.lo, shape.hi)))
    }

    /// A picture of the body of `look` wearing the appearance `table` with these keys and values.
    /// `None` when it draws nothing.
    pub fn worn_frame(&mut self, look: u32, table: u32, keys: &[(u32, f32)]) -> Option<Frame> {
        let mut desc = plain_desc(look);
        let values = Keyed::stored(WORN_BUCKETS, keys.to_vec());
        desc.global.appearance = Keyed::stored(WORN_BUCKETS, vec![(table, values)]);
        let dressed = Look::of(&self.anims, &desc).ok()?;
        let figure = Figure::from_look(&self.anims, &dressed)?;
        let poses = figure.pose(&self.anims);
        let place = Mat4::from_parts(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, dressed.scale);
        let bodies = self
            .drawer
            .batches(&self.data.records, &figure, &poses, &place);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for b in &bodies {
            for v in b.vertices.iter() {
                let q = b
                    .transform
                    .transform4([v.position[0], v.position[1], v.position[2], 1.0]);
                for ((lo, hi), v) in lo.iter_mut().zip(&mut hi).zip(q) {
                    *lo = lo.min(v);
                    *hi = hi.max(v);
                }
            }
        }
        (lo[0] <= hi[0]).then(|| self.picture_of(bodies, picture_camera(lo, hi)))
    }

    /// A frame of these bodies alone, in a plain light.
    fn picture_of(&mut self, bodies: Vec<ObjectBatch>, camera: Camera) -> Frame {
        let light = DayLight {
            ambient_brightness: 0.45,
            ambient_color: Color::WHITE,
            direct_brightness: 0.75,
            direct_color: Color::WHITE,
            light_direction: math::normalize(Vec3::new(0.3, -0.9, -0.8)),
            fog_min: 0.0,
            fog_max: 0.0,
            ..DayLight::DEFAULT
        };
        self.builder.parts_frame(&[], bodies, camera, light, None)
    }

    /// What the rows' effects draw now: each plays from when its row first stands here and stops
    /// when it is gone, standing where its row stands.
    fn row_effects_batches(
        &mut self,
        rows: &[Standing],
        block: (i32, i32),
        view: &View,
    ) -> Vec<ObjectBatch> {
        let now = self.started.elapsed().as_secs_f64();
        let records = &self.data.records;
        let mut seen: HashMap<u32, usize> = HashMap::new();
        let mut kept = HashSet::new();
        for r in rows {
            let Some(Some(p)) = self.looks.get(&r.look) else {
                continue;
            };
            let Some(look) = &p.with_effects else {
                continue;
            };
            let n = seen.entry(r.look).or_default();
            let key = (r.look, *n);
            *n += 1;
            kept.insert(key);
            let floor = dereth_primitives::num::floor_to_i32;
            let home = (floor(r.at[0] / BLOCK), floor(r.at[1] / BLOCK));
            #[allow(clippy::cast_precision_loss)]
            let place = Mat4::from_parts(
                Vec3::new(
                    r.at[0] - home.0 as f32 * BLOCK,
                    r.at[1] - home.1 as f32 * BLOCK,
                    r.at[2],
                ),
                Quat {
                    w: r.rotation[0],
                    x: r.rotation[1],
                    y: r.rotation[2],
                    z: r.rotation[3],
                },
                Vec3::new(1.0, 1.0, 1.0),
            );
            let scale = Vec3::new(
                p.scale.x * r.scale,
                p.scale.y * r.scale,
                p.scale.z * r.scale,
            );
            // A row carried into another block starts its effects again there.
            if self.row_effects.get(&key).is_some_and(|e| e.home != home) {
                self.row_effects.remove(&key);
            }
            let playing = match self.row_effects.entry(key) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let Some(mut effects) = ObjectEffects::of_look(look) else {
                        continue;
                    };
                    effects.start(&self.systems, records, home, &place, scale, now);
                    e.insert(RowEffects { effects, home })
                }
            };
            playing.effects.use_time(records, now);
            playing
                .effects
                .think(&self.systems, home, &place, scale, Color::WHITE, now);
        }
        self.row_effects.retain(|k, _| kept.contains(k));
        if kept.is_empty() {
            return Vec::new();
        }
        let f = view.forward;
        let forward = Vec3::new(f[0], f[1], f[2]);
        let right = math::normalize(Vec3::new(f[1], -f[0], 0.0));
        let up = Vec3::new(
            right.y * forward.z - right.z * forward.y,
            right.z * forward.x - right.x * forward.z,
            right.x * forward.y - right.y * forward.x,
        );
        #[allow(clippy::cast_precision_loss)]
        let eye = Vec3::new(
            view.eye[0] - block.0 as f32 * BLOCK,
            view.eye[1] - block.1 as f32 * BLOCK,
            view.eye[2],
        );
        self.systems
            .batches(block, BLOCK, now, eye, (right, forward, up))
    }

    /// The frame from `view`, with `rows` standing in it; `inside` the block and interior cell the
    /// camera is in, if it is in one.
    pub fn frame(&mut self, view: &View, rows: &[Standing], inside: Option<(u16, u16)>) -> &Frame {
        let floor = dereth_primitives::num::floor_to_i32;
        let block = match inside {
            Some((lb, _)) => {
                let [x, y] = lb.to_be_bytes();
                (i32::from(x), i32::from(y))
            }
            None => (floor(view.eye[0] / BLOCK), floor(view.eye[1] / BLOCK)),
        };
        #[allow(clippy::cast_precision_loss)]
        let origin = [block.0 as f32 * BLOCK, block.1 as f32 * BLOCK];
        let eye = Vec3::new(
            view.eye[0] - origin[0],
            view.eye[1] - origin[1],
            view.eye[2],
        );
        let cell = match inside {
            Some((_, low)) => (-1, i32::from(low)),
            None => (floor(eye.x / 24.0), floor(eye.y / 24.0)),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let moment = (self.time_of_day * 1000.0) as u32;
        let stale = self
            .built
            .as_ref()
            .is_none_or(|b| b.0 != block || b.1 != cell || b.2 != moment);
        if stale {
            let spot = Spot {
                block,
                position: eye,
                heading: view.heading(),
                pitch: view.pitch(),
                time_of_day: self.time_of_day,
                day: None,
            };
            let frame = match inside {
                Some((_, low)) => {
                    let (mut f, outside) = self.builder.inside_frame(&spot, self.fov, block, low);
                    let lights = self.builder.block_lights(block);
                    inside_light(&mut f.light, lights.as_deref(), eye, &outside);
                    f
                }
                None => self.builder.frame(&spot, self.fov),
            };
            let n = frame.objects.len();
            self.built = Some((block, cell, moment, frame, n));
        }
        for r in rows {
            let _ = self.posed(r.look);
        }
        let effects = self.row_effects_batches(rows, block, view);
        let (looks, drawer, records) = (&self.looks, &mut self.drawer, &self.data.records);
        let mut batches = Vec::new();
        for r in rows {
            let Some(Some(p)) = looks.get(&r.look) else {
                continue;
            };
            let place = Mat4::from_parts(
                Vec3::new(r.at[0] - origin[0], r.at[1] - origin[1], r.at[2]),
                Quat {
                    w: r.rotation[0],
                    x: r.rotation[1],
                    y: r.rotation[2],
                    z: r.rotation[3],
                },
                Vec3::new(
                    p.scale.x * r.scale,
                    p.scale.y * r.scale,
                    p.scale.z * r.scale,
                ),
            );
            batches.extend(drawer.batches(records, &p.figure, &p.poses, &place));
        }
        let Some(built) = self.built.as_mut() else {
            unreachable!("built above");
        };
        let frame = &mut built.3;
        frame.objects.truncate(built.4);
        frame.objects.extend(batches);
        frame.objects.extend(effects);
        frame.camera.position = eye;
        frame.camera.heading = view.heading();
        frame.camera.pitch = view.pitch();
        frame.camera.fov_y = self.fov;
        frame
    }
}
