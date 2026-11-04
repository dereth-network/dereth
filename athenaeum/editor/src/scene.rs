//! The world as the client draws it, seen from the free camera: the landscape, its scenery and
//! placed objects and the sky around the camera (rebuilt when the camera enters another cell),
//! and the world's rows standing in it as bodies, each put together from its template's look as
//! the client puts together what the server sends; and the ground under a point.

use std::collections::{HashMap, HashSet};

use auberean_animation::body::{Body as Figure, PartPose};
use auberean_animation::look::{Desc, Look, GLOBAL_KEY};
use auberean_animation::CachedRecords;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::{inside_light, Frame, SceneBuilder, Spot};
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;
use auberean_world::records::{RegionSet, WorldRecords};
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

/// A look put together once: its body at rest and its parts' poses.
struct Posed {
    figure: Figure,
    poses: Vec<PartPose>,
    scale: Vec3,
}

/// The world's frame as last built: for the camera's block, cell and moment of the day, with how
/// many of its objects are the world's own (the rows' bodies come after them).
type Built = ((i32, i32), (i32, i32), u32, Frame, usize);

/// The least and greatest corners of the box a look fills.
type LookBox = ([f32; 3], [f32; 3]);

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
    /// The box each look fills, for clicks on it.
    boxes: HashMap<u32, Option<LookBox>>,
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
            boxes: HashMap::new(),
        }
    }

    /// The ground's height under a point of the world's grid, `None` off the world.
    pub fn ground(&mut self, x: f32, y: f32) -> Option<f32> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (bx, by) = (
            (x / BLOCK).floor().clamp(0.0, 255.0) as u8,
            (y / BLOCK).floor().clamp(0.0, 255.0) as u8,
        );
        if self.loaded.insert((bx, by)) {
            self.land.add_block(
                &self.data.records,
                &self.data.region,
                &mut self.trees,
                bx,
                by,
            );
        }
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
            let desc = Desc {
                parent: DataId(look),
                scale: Vec3::new(1.0, 1.0, 1.0),
                child_scale: 1.0,
                particle_scale: 1.0,
                global: auberean_animation::look::Group {
                    key: GLOBAL_KEY,
                    ..Default::default()
                },
                ..Desc::default()
            };
            let posed = Look::of(&self.anims, &desc).ok().and_then(|l| {
                let figure = Figure::from_look(&self.anims, &l)?;
                let poses = figure.pose(&self.anims);
                Some(Posed {
                    figure,
                    poses,
                    scale: l.scale,
                })
            });
            self.looks.insert(look, posed);
        }
        self.looks.get(&look).is_some_and(Option::is_some)
    }

    /// The box a look fills when it stands at the origin unturned, at its own scale.
    pub fn look_box(&mut self, look: u32) -> Option<LookBox> {
        if let Some(known) = self.boxes.get(&look) {
            return *known;
        }
        let found = if self.posed(look) {
            let p = self.looks.get(&look).and_then(Option::as_ref);
            p.and_then(|p| {
                let place = Mat4::from_parts(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, p.scale);
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                let records = &self.data.records;
                for b in self.drawer.batches(records, &p.figure, &p.poses, &place) {
                    for v in b.vertices.iter() {
                        let at = b.transform.transform4([
                            v.position[0],
                            v.position[1],
                            v.position[2],
                            1.0,
                        ]);
                        for (i, c) in at.into_iter().take(3).enumerate() {
                            lo[i] = lo[i].min(c);
                            hi[i] = hi[i].max(c);
                        }
                    }
                }
                (lo[0] <= hi[0]).then_some((lo, hi))
            })
        } else {
            None
        };
        self.boxes.insert(look, found);
        found
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
        frame.camera.position = eye;
        frame.camera.heading = view.heading();
        frame.camera.pitch = view.pitch();
        frame.camera.fov_y = self.fov;
        frame
    }
}
