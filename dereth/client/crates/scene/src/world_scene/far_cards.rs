//! Cards for retail's own static objects far off: each object an art pack has no model for, drawn
//! from round it out of the dat files on threads of their own, so the far objects can draw it as a
//! card. The object with the most copies within reach is baked first. An object too small to be
//! seen far off, or that draws nothing, gets none.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, PoisonError};

use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;
use dereth_render::surface::surface_type;

use crate::art_pack::cards::{bake_parts, Card, Part, Texels};
use crate::art_pack::VERTEX_FLOATS;

/// An object smaller than this both across and up, metres, gets no card.
const LEAST_SIZE: f32 = 1.0;

/// A surface more see-through than this is left off a card.
const MOST_TRANSLUCENT: f32 = 0.6;

/// The cut a clip-mapped or alpha surface is drawn through.
const CUT: f32 = 0.5;

/// A retail object's card, in its own space, height up.
#[derive(Debug)]
pub(crate) struct RetailCard {
    /// The retail object: a setup or a graphics object.
    pub(crate) id: u32,
    /// Its pictures and its level, whose quad names its colour picture 0 and its normal 1.
    pub(crate) card: Card,
    /// A sphere holding it: its centre and radius.
    pub(crate) bounds: (glam::Vec3, f32),
}

/// One surface's triangles of an object, ready to draw onto its card.
struct Built {
    vertices: Vec<f32>,
    indices: Vec<u32>,
    picture: Option<DataId>,
    base_color: [f32; 4],
    cut: Option<f32>,
    double_sided: bool,
}

/// `id`'s surfaces, triangulated in its own space: its parts placed as it rests.
fn built(store: &RetailDatStore, id: DataId) -> Vec<Built> {
    let mut out = Vec::new();
    for part in dereth_client_runtime::models::resolve_parts(store, id) {
        let q = part.frame.rotation;
        let turn = glam::Quat::from_xyzw(q.x, q.y, q.z, q.w).normalize();
        let o = part.frame.origin;
        let origin = glam::Vec3::new(o.x, o.y, o.z);
        let scale = glam::Vec3::new(part.scale.x, part.scale.y, part.scale.z);
        for group in dereth_client_runtime::models::build_gfxobj(store, part.gfxobj) {
            let Some(surface) = group
                .surface
                .and_then(|s| super::appearance::read_surface(store, s))
            else {
                continue;
            };
            let kind = surface.surface_type;
            if kind & surface_type::ADDITIVE != 0 || surface.translucency > MOST_TRANSLUCENT {
                continue;
            }
            let pictured = kind & (surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP) != 0;
            let mut base_color = [1.0, 1.0, 1.0, 1.0 - surface.translucency.max(0.0)];
            if kind & surface_type::BASE1_SOLID != 0 && !pictured {
                let argb = surface.color_value.unwrap_or(0xFFFF_FFFF);
                for (c, shift) in [16u32, 8, 0].into_iter().enumerate() {
                    base_color[c] = linear(((argb >> shift) & 0xFF) as u8);
                }
            }
            let cut =
                (kind & (surface_type::BASE1_CLIPMAP | surface_type::ALPHA) != 0).then_some(CUT);
            let mut vertices = Vec::with_capacity(group.vertices.len() * VERTEX_FLOATS);
            for ((p, u, v), n) in group.vertices.iter().zip(&group.normals) {
                let at = origin + turn * (glam::Vec3::new(p.x, p.y, p.z) * scale);
                let up = (turn * glam::Vec3::new(n.x, n.y, n.z)).normalize_or_zero();
                vertices.extend([
                    at.x, at.y, at.z, up.x, up.y, up.z, 1.0, 0.0, 0.0, 1.0, *u, *v, 1.0, 1.0, 1.0,
                    1.0,
                ]);
            }
            let n = u32::try_from(group.vertices.len()).unwrap_or(0);
            out.push(Built {
                vertices,
                indices: (0..n).collect(),
                picture: pictured.then_some(group.surface).flatten(),
                base_color,
                cut,
                double_sided: group.two_sided,
            });
        }
    }
    out
}

/// The picture `surface` draws, four bytes a texel; `None` where it has none or will not decode.
fn picture(
    store: &RetailDatStore,
    textures: &crate::textures::TextureStore<'_>,
    surface: DataId,
) -> Option<Texels> {
    use dereth_primitives::TextureFormat as F;
    use dereth_render::pixel_format::PixelFormatId as P;
    let clip_map = super::appearance::read_surface(store, surface)
        .is_some_and(|s| s.surface_type & surface_type::BASE1_CLIPMAP != 0);
    let data = textures
        .texture_data_shifted(surface, clip_map, None)
        .ok()?;
    let level = data.levels.first()?;
    let (format, premultiplied) = match data.format {
        F::Bgra8 => (None, false),
        F::Bc1 => (Some(P::Dxt1), false),
        F::Bc2 => (Some(P::Dxt3), false),
        F::Bc2Premultiplied => (Some(P::Dxt2), true),
        F::Bc3 => (Some(P::Dxt5), false),
        F::Bc3Premultiplied => (Some(P::Dxt4), true),
        _ => return None,
    };
    let mut bgra = match format {
        Some(f) => dereth_render::dxt::decode(f, level, data.width, data.height).ok()?,
        None => level.clone(),
    };
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        let alpha = u32::from(px[3]);
        if premultiplied && alpha > 0 {
            for c in &mut px[..3] {
                *c = u8::try_from((u32::from(*c) * 255 / alpha).min(255)).unwrap_or(255);
            }
        }
    }
    Texels::new(data.width, data.height, bgra)
}

/// `id`'s card, its pictures read through `textures` and kept in `pictures`; `None` for an object
/// that draws nothing or is too small.
fn bake(
    store: &RetailDatStore,
    textures: &crate::textures::TextureStore<'_>,
    pictures: &mut HashMap<DataId, Option<Texels>>,
    id: u32,
) -> Option<RetailCard> {
    let built = built(store, DataId(id));
    if built.is_empty() {
        return None;
    }
    let (mut half, mut foot, mut top) = (0.0f32, f32::MAX, f32::MIN);
    for b in &built {
        for v in b.vertices.as_chunks::<VERTEX_FLOATS>().0 {
            half = half.max(dereth_primitives::num::math::hypotf(v[0], v[1]));
            foot = foot.min(v[2]);
            top = top.max(v[2]);
        }
    }
    if 2.0 * half < LEAST_SIZE && top - foot < LEAST_SIZE {
        return None;
    }
    for b in &built {
        if let Some(s) = b.picture {
            pictures
                .entry(s)
                .or_insert_with(|| picture(store, textures, s));
        }
    }
    let parts: Vec<Part<'_>> = built
        .iter()
        .map(|b| Part {
            vertices: &b.vertices,
            indices: &b.indices,
            picture: b
                .picture
                .and_then(|s| pictures.get(&s))
                .and_then(Option::as_ref),
            base_color: b.base_color,
            cut: b.cut,
            double_sided: b.double_sided,
            roughness: 0.9,
        })
        .collect();
    let card = bake_parts(&parts, false)?;
    let middle = (foot + top) * 0.5;
    let reach = (half * half + (top - middle) * (top - middle)).sqrt();
    Some(RetailCard {
        id,
        card,
        bounds: (glam::Vec3::new(0.0, 0.0, middle), reach),
    })
}

fn linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// The ids still to bake, each with how many copies of it are within reach, and the signal that
/// more are asked for.
type Queue = Arc<(Mutex<HashMap<u32, u32>>, Condvar)>;

/// The threads that bake the cards: a share of the machine's, two to four.
fn threads() -> usize {
    std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .div_ceil(3)
        .clamp(2, 4)
}

/// The id with the most copies, taken from `waiting`; the lowest id among equals.
fn most_copies(waiting: &mut HashMap<u32, u32>) -> Option<u32> {
    let id = waiting
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map(|(id, _)| *id)?;
    waiting.remove(&id);
    Some(id)
}

/// The threads that bake the cards, and what they share with the frame.
struct Worker {
    queue: Queue,
    done: mpsc::Receiver<(u32, Option<RetailCard>)>,
    stop: Arc<AtomicBool>,
}

impl Worker {
    fn start(store: &Arc<RetailDatStore>) -> Option<Self> {
        let queue: Queue = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (send, done) = mpsc::channel();
        let mut started = 0;
        for _ in 0..threads() {
            let (q, halt, send, store) = (
                Arc::clone(&queue),
                Arc::clone(&stop),
                send.clone(),
                Arc::clone(store),
            );
            let spawned = crate::workers::spawn("far cards", move || {
                let textures = crate::textures::TextureStore::new(&store);
                let mut pictures = HashMap::new();
                loop {
                    let next = {
                        let (lock, wake) = &*q;
                        let mut waiting = lock.lock().unwrap_or_else(PoisonError::into_inner);
                        while waiting.is_empty() && !halt.load(Ordering::Relaxed) {
                            waiting = wake.wait(waiting).unwrap_or_else(PoisonError::into_inner);
                        }
                        most_copies(&mut waiting)
                    };
                    if halt.load(Ordering::Relaxed) {
                        return;
                    }
                    let Some(id) = next else {
                        continue;
                    };
                    let card = bake(&store, &textures, &mut pictures, id);
                    if send.send((id, card)).is_err() {
                        return;
                    }
                }
            });
            started += usize::from(spawned.is_ok());
        }
        (started > 0).then_some(Self { queue, done, stop })
    }

    fn busy(&self) -> bool {
        !self
            .queue
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.queue.1.notify_all();
    }
}

/// The retail cards baked so far, and the thread baking more.
#[derive(Default)]
pub(crate) struct FarCards {
    worker: Option<Worker>,
    /// The cards made, in the order they were made.
    pub(crate) made: Vec<Arc<RetailCard>>,
    /// The ids with a card.
    pub(crate) carded: HashSet<u32>,
    /// The ids asked for, carded, baking or waiting.
    asked: HashSet<u32>,
    /// How many are still baking.
    pending: usize,
}

impl std::fmt::Debug for FarCards {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FarCards")
            .field("made", &self.made.len())
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl FarCards {
    /// Bake the cards of `ids`, a copy each, on threads started with `store`: each id not asked
    /// for before waits with its copies counted, and an id still waiting counts these copies too.
    pub(crate) fn ask(&mut self, store: &Arc<RetailDatStore>, ids: impl IntoIterator<Item = u32>) {
        let mut ids = ids.into_iter().peekable();
        if ids.peek().is_none() {
            return;
        }
        if self.worker.is_none() {
            self.worker = Worker::start(store);
        }
        let Some(w) = self.worker.as_ref() else {
            return;
        };
        let (lock, wake) = &*w.queue;
        let mut waiting = lock.lock().unwrap_or_else(PoisonError::into_inner);
        for id in ids {
            if self.asked.insert(id) {
                self.pending += 1;
                waiting.insert(id, 1);
            } else if let Some(copies) = waiting.get_mut(&id) {
                *copies += 1;
            }
        }
        wake.notify_all();
    }

    /// The ids whose cards were made since the last call.
    pub(crate) fn take(&mut self) -> Vec<u32> {
        let Some(w) = self.worker.as_ref() else {
            return Vec::new();
        };
        let mut fresh = Vec::new();
        while let Ok((id, card)) = w.done.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            if let Some(card) = card {
                self.carded.insert(id);
                self.made.push(Arc::new(card));
                fresh.push(id);
            }
        }
        fresh
    }

    /// Wait until every card asked for is baked: for a capture that draws them from its first
    /// frames.
    pub(crate) fn wait(&mut self) -> Vec<u32> {
        let mut fresh = Vec::new();
        let started = web_time::Instant::now();
        while self.pending > 0 || self.worker.as_ref().is_some_and(Worker::busy) {
            let Some(w) = self.worker.as_ref() else {
                break;
            };
            match w.done.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok((id, card)) => {
                    self.pending = self.pending.saturating_sub(1);
                    if let Some(card) = card {
                        self.carded.insert(id);
                        self.made.push(Arc::new(card));
                        fresh.push(id);
                    }
                }
                Err(_) => break,
            }
            if started.elapsed() > std::time::Duration::from_secs(300) {
                break;
            }
        }
        fresh
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi far objects)
    use super::*;

    #[test]
    fn the_id_with_the_most_copies_within_reach_is_baked_first() {
        let mut waiting: HashMap<u32, u32> = [(10, 3), (11, 40), (12, 40), (13, 1)].into();
        assert_eq!(
            most_copies(&mut waiting),
            Some(11),
            "the lower id among equals"
        );
        assert_eq!(most_copies(&mut waiting), Some(12));
        assert_eq!(most_copies(&mut waiting), Some(10));
        assert_eq!(most_copies(&mut waiting), Some(13));
        assert_eq!(most_copies(&mut waiting), None);
        assert!((2..=4).contains(&threads()));
    }
}
