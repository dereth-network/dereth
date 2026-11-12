//! The remaster's pictures made ahead of the changes that hold them, off the frame.
//!
//! A block or an object coming into the remaster can bring dozens of pictures it does not hold
//! yet, and making each (decoding it, scaling it and building its levels) takes milliseconds.
//! While the landscape streams over several frames (a connected client), the pictures a change
//! wants are made on a thread of their own, exactly as the frame would make them, and the change
//! waits for them; the frame then only takes them. An offline run or a capture makes them on the
//! frame, as it always has.

use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};

use dereth_dat::RetailDatStore;
use dereth_primitives::TextureData;
use dereth_remaster::assets::pack::PackSettings;
use dereth_remaster::assets::{AssetSource as _, TextureKey};
use dereth_render::texture::ImageScale;

use super::remaster_assets::DatAssets;
use super::remaster_pack::PackAssets;

/// How many frames a picture made ahead is kept for the change that asked for it, before it is
/// let go as one that will not come.
const KEPT_FRAMES: u64 = 600;

/// What a picture is made under: the files, the texture detail and scale, and the art pack and
/// what of it is drawn. Pictures made under other settings are let go.
#[derive(Clone)]
pub(super) struct Making {
    pub(super) store: RetailDatStore,
    pub(super) detail: u32,
    pub(super) scale: ImageScale,
    pub(super) pack: Option<(Arc<PackAssets>, PackSettings)>,
}

impl Making {
    /// Whether pictures made under `self` and `other` are the same.
    fn same(&self, other: &Self) -> bool {
        self.detail == other.detail
            && self.scale == other.scale
            && match (&self.pack, &other.pack) {
                (None, None) => true,
                (Some((a, x)), Some((b, y))) => Arc::ptr_eq(a, b) && x == y,
                _ => false,
            }
    }
}

/// A picture asked for, the generation of the settings it is made under, and those settings.
type Job = ((TextureKey, bool), u64, Making);
/// A picture made, for that generation; `None` where it will not decode.
type Done = ((TextureKey, bool), u64, Option<TextureData>);

/// The pictures made ahead: those asked for, those made and not yet taken, and the thread.
#[derive(Default)]
pub(crate) struct Pictures {
    worker: Option<(mpsc::Sender<Job>, mpsc::Receiver<Done>)>,
    tried: bool,
    making: Option<Making>,
    generation: u64,
    asked: HashSet<(TextureKey, bool)>,
    // ORDER-OK: looked up by key only.
    made: HashMap<(TextureKey, bool), (Option<TextureData>, u64)>,
    frame: u64,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pictures")
            .field("asked", &self.asked.len())
            .field("made", &self.made.len())
            .finish_non_exhaustive()
    }
}

/// Whether a picture is made by [`Pictures`]: the dats' and the art pack's own, not the ground's,
/// which are made from the region.
pub(super) fn made_ahead(key: &TextureKey) -> bool {
    matches!(key, TextureKey::Dat { .. } | TextureKey::Shifted { .. })
}

impl Pictures {
    /// A new frame, its pictures made under `making`: what the thread has made is taken in, and
    /// what was made long ago and never taken, or under other settings, is let go.
    pub(super) fn frame(&mut self, making: Making) {
        self.frame += 1;
        if !self.making.as_ref().is_some_and(|m| m.same(&making)) {
            self.generation += 1;
            self.asked.clear();
            self.made.clear();
            self.making = Some(making);
        }
        if let Some((_, done)) = self.worker.as_ref() {
            while let Ok((key, generation, picture)) = done.try_recv() {
                if generation == self.generation && self.asked.remove(&key) {
                    self.made.insert(key, (picture, self.frame));
                }
            }
        }
        let frame = self.frame;
        self.made
            .retain(|_, (_, at)| frame.saturating_sub(*at) < KEPT_FRAMES);
    }

    /// Whether picture `key` is made and ready to be taken, asking the thread to make it if it
    /// has not been. Ready, too, where no thread could be started: the frame makes it.
    pub(super) fn prepare(&mut self, key: &TextureKey, clip_map: bool) -> bool {
        let k = (key.clone(), clip_map);
        if self.made.contains_key(&k) {
            return true;
        }
        if !self.tried {
            self.tried = true;
            self.worker = start();
        }
        let (Some((jobs, _)), Some(making)) = (self.worker.as_ref(), self.making.as_ref()) else {
            return true;
        };
        if self.asked.contains(&k) {
            return false;
        }
        if jobs
            .send((k.clone(), self.generation, making.clone()))
            .is_err()
        {
            self.worker = None;
            return true;
        }
        self.asked.insert(k);
        false
    }

    /// Picture `key`, when it was made ahead: taken, so it is made only once.
    pub(super) fn take(&mut self, key: &TextureKey, clip_map: bool) -> Option<Option<TextureData>> {
        self.made
            .remove(&(key.clone(), clip_map))
            .map(|(picture, _)| picture)
    }
}

/// The thread that makes the pictures; `None` where it cannot be started.
fn start() -> Option<(mpsc::Sender<Job>, mpsc::Receiver<Done>)> {
    let (jobs, asked) = mpsc::channel::<Job>();
    let (send, done) = mpsc::channel::<Done>();
    crate::workers::spawn("remaster pictures", move || {
        for ((key, clip_map), generation, making) in asked {
            let (pack, on) = making
                .pack
                .as_ref()
                .map_or((None, PackSettings::default()), |(p, on)| {
                    (Some(Arc::clone(p)), *on)
                });
            let mut src =
                DatAssets::new(&making.store, making.detail, making.scale).with_pack(pack, on);
            let picture = src.texture(&key, clip_map);
            if send.send(((key, clip_map), generation, picture)).is_err() {
                return;
            }
        }
    })
    .ok()?;
    Some((jobs, done))
}
