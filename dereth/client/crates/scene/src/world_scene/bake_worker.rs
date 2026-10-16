//! A landblock's objects baked off the frame.
//!
//! While the landscape streams over several frames (a connected client), the part of a block's
//! bake that needs no device is made on a thread of its own: the block's mesh, what stands on it
//! ([`land::place_objects`]) and every placement's triangles gathered by surface
//! ([`land::gather_objects`]), which together are most of a bake's time. The frame finishes it:
//! the pictures and pipeline states the surfaces resolve to, the interior cells and the
//! buildings' openings ([`LandContext::finish_objects`]). Both sides run the same functions as a
//! bake on the frame does, so a block baked off the frame is the block baked on it; an offline run
//! or a capture bakes on the frame.
//!
//! The thread also decodes the pictures of the block's surfaces the frame has not resolved yet,
//! as texture resolution decodes them, with their compressed mip chains, and works out each
//! surface's bounding sphere: the frame resolves the surfaces from those.
//!
//! A bake is made for what it was asked under ([`BakeKey`]): the block's detail, the land's
//! inputs and the switches. One that comes back for anything else is thrown away and asked for
//! again.

use std::sync::{mpsc, Condvar, Mutex, PoisonError};

use super::land::{self, ObjectKnobs, PreparedObjects};
use super::*;

/// What every bake off the frame is made from: the files, the land's region and ground, and the
/// switches. Made again whenever any of them changes.
pub(super) struct BakeFrom {
    /// [`LandContext`]'s `bake_epoch` it was made for.
    pub(super) epoch: u64,
    pub(super) store: Arc<RetailDatStore>,
    pub(super) region: Arc<dereth_assets::Region>,
    /// The region whose land surface draws the ground, when it is not [`Self::region`]'s.
    pub(super) ground: Option<Arc<dereth_assets::Region>>,
    /// The terrain types the ground has a picture for.
    pub(super) drawn: u32,
    pub(super) table: Box<[f32; dereth_terrain::consts::LAND_HEIGHT_TABLE_LEN]>,
    pub(super) knobs: ObjectKnobs,
    /// The texture detail and image scale the surfaces' pictures are decoded at, and whether
    /// the files held their high-resolution pictures.
    pub(super) pictures: (u32, dereth_render::texture::ImageScale, bool),
}

/// What a bake off the frame was made for: compared with what the frame would bake the block for
/// now before it is taken.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BakeKey {
    pub(super) spec: SlotSpec,
    /// The land's inputs' generation ([`LandContext`]'s `bake_epoch`).
    pub(super) epoch: u64,
    pub(super) knobs: ObjectKnobs,
}

/// One block asked for.
pub(super) struct BakeJob {
    pub(super) key: BakeKey,
    pub(super) from: Arc<BakeFrom>,
    /// The surfaces the frame has resolved already: their pictures are not decoded again.
    pub(super) known: Arc<HashSet<DataId>>,
}

/// One block baked off the frame, as far as it is without the device.
pub(super) struct BakeDone {
    pub(super) key: BakeKey,
    pub(super) lb: CellLandblock,
    /// The record as the ground's land surface draws it, where a terrain type it has no picture
    /// for was filled.
    pub(super) filled: Option<CellLandblock>,
    /// The mesh, before its vertex lighting: the frame lights it with the light it has then.
    pub(super) mesh: LandblockMesh,
    pub(super) objects: PreparedObjects,
}

/// What the thread shares with the frame: the blocks asked for and not started, and the viewer's
/// block, the nearest to which is started first.
struct Queue {
    jobs: Vec<BakeJob>,
    viewer: (i32, i32),
    stop: bool,
}

/// The thread that bakes blocks' objects off the frame.
pub(super) struct BakeWorker {
    shared: Arc<(Mutex<Queue>, Condvar)>,
    done: mpsc::Receiver<(SlotSpec, Option<BakeDone>)>,
}

impl std::fmt::Debug for BakeWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BakeWorker").finish_non_exhaustive()
    }
}

impl BakeWorker {
    /// Start the thread; `None` where it cannot be started, and blocks are baked on the frame.
    pub(super) fn start() -> Option<Self> {
        let shared = Arc::new((
            Mutex::new(Queue {
                jobs: Vec::new(),
                viewer: (0, 0),
                stop: false,
            }),
            Condvar::new(),
        ));
        let (send, done) = mpsc::channel();
        let queue = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("landblock bakes".to_owned())
            .spawn(move || run(&queue, &send))
            .ok()?;
        Some(Self { shared, done })
    }

    /// Bake `job`'s block too, in place of any job for it still waiting, with the viewer in block
    /// `viewer`.
    pub(super) fn ask(&self, job: BakeJob, viewer: (i32, i32)) {
        let (lock, wake) = &*self.shared;
        let mut q = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let block = (job.key.spec.block_x, job.key.spec.block_y);
        q.jobs
            .retain(|j| (j.key.spec.block_x, j.key.spec.block_y) != block);
        q.jobs.push(job);
        q.viewer = viewer;
        wake.notify_one();
    }

    /// Drop the jobs still waiting that `keep` does not keep, with the viewer in block `viewer`.
    pub(super) fn keep(&self, viewer: (i32, i32), keep: impl Fn((i32, i32)) -> bool) {
        let mut q = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        q.viewer = viewer;
        q.jobs
            .retain(|j| keep((j.key.spec.block_x, j.key.spec.block_y)));
    }

    /// A block the thread is done with: its spec, and the bake, or `None` for a block whose
    /// record the files do not hold.
    pub(super) fn try_take(&self) -> Option<(SlotSpec, Option<BakeDone>)> {
        self.done.try_recv().ok()
    }
}

impl Drop for BakeWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        wake.notify_all();
    }
}

/// The thread: the job nearest the viewer, baked, until told to stop or the frame's side is gone.
/// Its decode memo is its own, kept for its life.
fn run(shared: &(Mutex<Queue>, Condvar), send: &mpsc::Sender<(SlotSpec, Option<BakeDone>)>) {
    let mut cache = BakeCache::default();
    loop {
        let job = {
            let (lock, wake) = shared;
            let mut q = lock.lock().unwrap_or_else(PoisonError::into_inner);
            while q.jobs.is_empty() && !q.stop {
                q = wake.wait(q).unwrap_or_else(PoisonError::into_inner);
            }
            if q.stop {
                return;
            }
            let viewer = q.viewer;
            let apart = |j: &BakeJob| {
                let (dx, dy) = (j.key.spec.block_x - viewer.0, j.key.spec.block_y - viewer.1);
                (dx.abs().max(dy.abs()), dx * dx + dy * dy)
            };
            let nearest = (0..q.jobs.len())
                .min_by_key(|&i| (apart(&q.jobs[i]), i))
                .unwrap_or(0);
            q.jobs.swap_remove(nearest)
        };
        let spec = job.key.spec;
        if send.send((spec, bake(job, &mut cache))).is_err() {
            return;
        }
    }
}

/// `job`'s block baked as far as it is without the device, through `cache`; `None` for a block
/// whose record the files do not hold.
fn bake(job: BakeJob, cache: &mut BakeCache) -> Option<BakeDone> {
    let from = &*job.from;
    let spec = job.key.spec;
    let (bx, by) = (spec.block_x, spec.block_y);
    let lb = read_landblock(&from.store, bx, by)?;
    let (mesh, filled) = land::landblock_mesh(
        &lb,
        from.drawn,
        from.ground.as_deref().unwrap_or(&from.region),
        &from.table,
        spec,
    );
    let placed = land::place_objects(
        &from.store,
        &from.region,
        (&lb, &mesh, bx, by),
        from.knobs,
        &|_, _| false,
    );
    let mut gathered = land::gather_objects(&from.store, cache, None, &placed, from.knobs);
    let mut surfaces = Vec::new();
    for (_, assembly) in &mut gathered.sides {
        assembly.work_out_spheres();
        surfaces.extend(assembly.surfaces());
    }
    surfaces.sort_unstable_by_key(|id| id.0);
    surfaces.dedup();
    let textures = surfaces
        .into_iter()
        .filter(|s| !job.known.contains(s))
        .filter_map(|s| prefetch(from, s).map(|t| (s, Arc::new(t))))
        .collect();
    Some(BakeDone {
        key: job.key,
        lb,
        filled,
        mesh,
        objects: PreparedObjects {
            placed,
            gathered,
            objects_visual: false,
            textures,
        },
    })
}

/// Surface `surface`'s picture, decoded as texture resolution decodes it for a surface with no
/// appearance of its own: scaled to the image scale, and a compressed picture's mip chain built.
/// `None` where it has no picture, or it does not decode: the frame then does as it would.
fn prefetch(from: &BakeFrom, surface: DataId) -> Option<PrefetchedTexture> {
    let (detail, scale, _) = from.pictures;
    let textures = TextureStore::with_environment_texture_detail(&from.store, detail);
    let clip_map = read_surface(&from.store, surface)?.surface_type
        & dereth_render::surface::surface_type::BASE1_CLIPMAP
        != 0;
    let data = textures
        .texture_data_shifted(surface, clip_map, None)
        .ok()?;
    let scaled = dereth_render::texture::scale_surface(&data, scale).ok()?;
    let (key, _) = combined_key(&textures, Some(surface), None);
    let chain = if crate::mip_worker::MipWorker::needs_chain(&scaled) && !key.is_uncached() {
        dereth_render::mip::compressed_system_chain(&scaled)
            .ok()
            .flatten()
            // LINT-OK: the image scale's shift count, as texture resolution casts it.
            .map(|c| ((key.raw(), scale as u32), Arc::new(c)))
    } else {
        None
    };
    Some(PrefetchedTexture {
        source: (data.width, data.height),
        data: scaled,
        chain,
        made_at: (detail, scale),
    })
}

/// What a block's build does with its objects.
pub(super) enum OffFrame {
    /// Baked off the frame: the build finishes them.
    Ready(Box<BakeDone>),
    /// Asked of the thread and not done: the block waits.
    Wait,
    /// Baked on the frame, by the build.
    OnFrame,
}

impl SceneDraw {
    /// The switches the blocks' objects are baked with now.
    pub(super) fn object_knobs(&self, gpu: &Gpu) -> ObjectKnobs {
        #[cfg(not(feature = "hifi"))]
        let _ = gpu;
        ObjectKnobs {
            part_degrades: self.cfg.part_degrades,
            degrade_levels: self.cfg.degrade_levels,
            static_billboards: self.cfg.static_billboards,
            lod_object_guard: self.cfg.lod_object_guard,
            #[cfg(feature = "hifi")]
            lamps: super::hifi_bridge::lamps_possible(gpu),
        }
    }

    /// Take the blocks the thread is done with, and forget, there and here, the blocks no longer
    /// queued.
    pub(super) fn take_bakes(&mut self, ws: &WorldState) {
        let Some(worker) = self.land.bakes.as_ref() else {
            return;
        };
        while let Some((spec, done)) = worker.try_take() {
            let block = (spec.block_x, spec.block_y);
            let asked = self.land.bakes_asked.get(&block);
            match done {
                Some(done) if asked == Some(&done.key) => {
                    self.land.bakes_done.insert(block, done);
                }
                None if asked.is_some_and(|k| k.spec == spec) => {
                    self.land.bakes_asked.remove(&block);
                    self.land.bakes_on_frame.insert(block);
                }
                // Asked again for something else since.
                _ => {}
            }
        }
        let pending = &self.pending;
        self.land.bakes_asked.retain(|b, _| pending.contains_key(b));
        self.land.bakes_done.retain(|b, _| pending.contains_key(b));
        self.land.bakes_on_frame.retain(|b| pending.contains_key(b));
        let viewer = ws.streamer.window.viewer_block().unwrap_or((0, 0));
        worker.keep(viewer, |b| pending.contains_key(&b));
    }

    /// What block `block`'s build, `work`, does with its objects. While the landscape streams
    /// over several frames, a build that bakes the block's objects with the world's own look
    /// takes them baked off the frame: ready when the thread has them for what the block would be
    /// baked for now, and asked for otherwise while the block waits. Anything else bakes on the
    /// frame, the viewer's own block among it: a window built round the viewer (a load, a
    /// teleport) stands the camera and the body on that block at once.
    pub(super) fn bake_off_frame(
        &mut self,
        ws: &WorldState,
        store: &RetailDatStore,
        gpu: &Gpu,
        block: (i32, i32),
        work: SlotWork,
    ) -> OffFrame {
        if self.cfg.stream_budget.is_none()
            || self.land.objects.is_some()
            || self.land.bakes_failed
            || self.land.bakes_on_frame.remove(&block)
            || ws.streamer.window.viewer_block() == Some(block)
        {
            return OffFrame::OnFrame;
        }
        let Some((xi, yi)) = self.slot_of_block(ws, block) else {
            return OffFrame::OnFrame;
        };
        // A re-mesh keeps the objects of a block already baked.
        let keeps = work == SlotWork::Mesh && self.blocks.get(&block).is_some_and(|b| b.baked);
        let Some(slot) = ws.streamer.window.slot(xi, yi) else {
            return OffFrame::OnFrame;
        };
        if keeps || !self.wants_objects(ws, xi, yi) {
            return OffFrame::OnFrame;
        }
        let spec = SlotSpec {
            block_x: slot.block_x,
            block_y: slot.block_y,
            lod_div: slot.lod_div,
            dir: slot.trans_dir,
        };
        let knobs = self.object_knobs(gpu);
        let key = BakeKey {
            spec,
            epoch: self.land.bake_epoch,
            knobs,
        };
        if let Some(done) = self.land.bakes_done.remove(&block) {
            if done.key == key {
                self.land.bakes_asked.remove(&block);
                return OffFrame::Ready(Box::new(done));
            }
        }
        if self.land.bakes_asked.get(&block) == Some(&key) {
            return OffFrame::Wait;
        }
        if self.land.bakes.is_none() {
            self.land.bakes = BakeWorker::start();
            self.land.bakes_failed = self.land.bakes.is_none();
        }
        let from = self.bake_from(store, knobs);
        let Some(worker) = self.land.bakes.as_ref() else {
            return OffFrame::OnFrame;
        };
        let viewer = ws.streamer.window.viewer_block().unwrap_or(block);
        let known = self
            .land
            .bake
            .surfaces
            .keys()
            .filter(|k| k.appearance == SurfaceAppearance::default())
            .filter_map(|k| k.surface)
            .collect();
        worker.ask(
            BakeJob {
                key: key.clone(),
                from,
                known: Arc::new(known),
            },
            viewer,
        );
        self.land.bakes_asked.insert(block, key);
        OffFrame::Wait
    }

    /// What bakes off the frame are made from now, made again when it has changed.
    fn bake_from(&mut self, store: &RetailDatStore, knobs: ObjectKnobs) -> Arc<BakeFrom> {
        let land = &mut self.land;
        let pictures = (
            land.bake.environment_texture_detail,
            land.bake.image_scale,
            store.highres_granted(),
        );
        if let Some(from) = land
            .bake_from
            .as_ref()
            .filter(|f| f.epoch == land.bake_epoch && f.knobs == knobs && f.pictures == pictures)
        {
            return Arc::clone(from);
        }
        let from = Arc::new(BakeFrom {
            epoch: land.bake_epoch,
            store: Arc::new(store.clone()),
            region: Arc::new((*land.region).clone()),
            ground: land.ground.as_ref().map(|g| Arc::new((**g).clone())),
            drawn: land.drawn,
            table: land.table.clone(),
            knobs,
            pictures,
        });
        land.bake_from = Some(Arc::clone(&from));
        from
    }

    /// How many blocks' objects have been baked on the frame, and how many off it and finished
    /// on it.
    #[must_use]
    pub fn objects_baked(&self) -> (u64, u64) {
        self.land.objects_baked
    }
}
