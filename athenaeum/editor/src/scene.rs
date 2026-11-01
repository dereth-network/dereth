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
use auberean_draw::scene::{Frame, SceneBuilder, Spot};
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_primitives::LandblockId;

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

    /// The frame from `view`, with `rows` standing in it.
    pub fn frame(&mut self, view: &View, rows: &[Standing]) -> &Frame {
        let floor = dereth_primitives::num::floor_to_i32;
        let block = (floor(view.eye[0] / BLOCK), floor(view.eye[1] / BLOCK));
        #[allow(clippy::cast_precision_loss)]
        let origin = [block.0 as f32 * BLOCK, block.1 as f32 * BLOCK];
        let eye = Vec3::new(
            view.eye[0] - origin[0],
            view.eye[1] - origin[1],
            view.eye[2],
        );
        let cell = (floor(eye.x / 24.0), floor(eye.y / 24.0));
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
            let frame = self.builder.frame(&spot, self.fov);
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
