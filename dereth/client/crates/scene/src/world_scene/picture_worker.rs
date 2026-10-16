//! The ground's pictures the splat draw blends, decoded off the frame.
//!
//! The first block to need a terrain picture or blend mask decodes it, a few milliseconds each,
//! and a block entering the window at its edge often needs several the window has not seen.
//! While the landscape streams over several frames (a connected client), the pictures a block's
//! splat layers need are decoded on a thread of their own, as the frame would decode them, and
//! the block waits for them; the frame then only uploads them. An offline run or a capture
//! decodes them on the frame.

use std::sync::mpsc;

use super::*;

/// A picture asked for: its id, the generation of the decoded pictures it is for, the files and
/// the texture detail it is read with.
type Job = (DataId, u64, Arc<RetailDatStore>, u32);
/// A picture decoded, for that generation; `None` where the files hold none.
type Done = (DataId, u64, Option<Arc<Bgra8>>);

/// The thread that decodes the splat's pictures.
#[derive(Debug)]
pub(super) struct PictureWorker {
    jobs: mpsc::Sender<Job>,
    done: mpsc::Receiver<Done>,
}

impl PictureWorker {
    /// Start the thread; `None` where it cannot be started, and the pictures are decoded on the
    /// frame.
    pub(super) fn start() -> Option<Self> {
        let (jobs, asked) = mpsc::channel::<Job>();
        let (send, done) = mpsc::channel::<Done>();
        std::thread::Builder::new()
            .name("ground pictures".to_owned())
            .spawn(move || {
                for (id, generation, store, detail) in asked {
                    let textures = TextureStore::with_environment_texture_detail(&store, detail);
                    let picture = textures.bgra8(id).ok().map(Arc::new);
                    if send.send((id, generation, picture)).is_err() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self { jobs, done })
    }

    /// Decode `id` from `store` at texture detail `detail`, for the pictures' generation
    /// `generation`; `false` when the thread is gone.
    pub(super) fn ask(
        &self,
        id: DataId,
        generation: u64,
        store: Arc<RetailDatStore>,
        detail: u32,
    ) -> bool {
        self.jobs.send((id, generation, store, detail)).is_ok()
    }

    /// A picture the thread is done with.
    pub(super) fn try_take(&self) -> Option<Done> {
        self.done.try_recv().ok()
    }
}

impl LandContext {
    /// The pictures `mesh`'s splat layers need that are neither held for the splat draw nor
    /// decoded, while the landscape streams over several frames: each asked of the picture
    /// thread once. Empty when the block can be built now: nothing is missing, the ground is not
    /// splatted, or no thread could be started.
    pub(super) fn splat_pictures_waiting(
        &mut self,
        store: &RetailDatStore,
        mesh: &LandblockMesh,
    ) -> Vec<DataId> {
        if !self.splat_wanted || self.pal_shift.is_some() || self.pictures_failed {
            return Vec::new();
        }
        let mut missing: Vec<DataId> = Vec::new();
        for (key, _) in &mesh.cell_keys {
            if self.splats.contains_key(key) {
                continue;
            }
            let plan =
                dereth_terrain::land::merge::merge_plan(&self.tex_merge, *key, self.merge.shift);
            let ids = plan.base.into_iter().chain(
                plan.overlays
                    .iter()
                    .flat_map(|o| std::iter::once(o.alpha).chain(o.tex)),
            );
            for id in ids {
                if !self.splat_sources.contains_key(&id)
                    && !self.sources.contains_key(&id)
                    && !missing.contains(&id)
                {
                    missing.push(id);
                }
            }
        }
        if missing.is_empty() {
            return missing;
        }
        if self.pictures.is_none() {
            self.pictures = PictureWorker::start();
            self.pictures_failed = self.pictures.is_none();
        }
        let Some(worker) = self.pictures.as_ref() else {
            return Vec::new();
        };
        let files = Arc::new(self.ground_store.clone().unwrap_or_else(|| store.clone()));
        for id in &missing {
            if self.pictures_asked.insert(*id)
                && !worker.ask(
                    *id,
                    self.pictures_epoch,
                    Arc::clone(&files),
                    self.bake.environment_texture_detail,
                )
            {
                self.pictures_failed = true;
                return Vec::new();
            }
        }
        missing
    }

    /// Take the pictures the thread has decoded for the pictures held now.
    pub(super) fn take_pictures(&mut self) {
        let Some(worker) = self.pictures.as_ref() else {
            return;
        };
        while let Some((id, generation, picture)) = worker.try_take() {
            if generation != self.pictures_epoch || !self.pictures_asked.remove(&id) {
                continue;
            }
            self.pictures_decoded.1 += 1;
            self.sources.entry(id).or_insert(picture);
        }
    }

    /// Whether picture `id` is decoded, or held for the splat draw.
    pub(super) fn picture_ready(&self, id: DataId) -> bool {
        self.sources.contains_key(&id) || self.splat_sources.contains_key(&id)
    }

    /// Throw away what the thread is asked for: the pictures decoded are being thrown away too.
    pub(super) fn forget_pictures(&mut self) {
        self.pictures_epoch += 1;
        self.pictures_asked.clear();
    }
}
