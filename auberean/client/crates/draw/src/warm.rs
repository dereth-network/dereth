//! The world readied ahead of the frame that shows it: while portal space still hides the world,
//! what the world's first frame will put on the device is handed to it a share a frame (its
//! pictures, then its draws' lists, texture sets and programs, its shadow casters' and the land's
//! lists, and the pictures it is drawn into), so the frame that shows the world finds them there
//! instead of making them all at once.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use crate::constants::bound_stages;
use crate::material::DrawLayer;
use crate::scene::{Frame, ObjectBatch, TerrainBatch};

/// How many pictures are handed out a frame: a town's few hundred over a fraction of portal
/// space's last second.
pub const PER_FRAME: usize = 12;

/// How many draws (and as many shadow casters, and land pieces) are handed out a frame, once the
/// pictures are: a town's thousand or so over part of the same second.
pub const DRAWS_PER_FRAME: usize = 48;

/// The pictures a draw of `layer` binds in its three texture slots, and whether each clamps: 0
/// for none.
#[must_use]
pub fn texture_set(layer: &DrawLayer) -> [(u32, bool); 3] {
    bound_stages(layer).map(|s| {
        (
            s.map_or(0, |s| s.texture),
            s.is_some_and(|s| s.clamp_u || s.clamp_v),
        )
    })
}

/// The pictures a frame is drawn into besides its own color and depth, which are always made:
/// the water's reflection's, the ripples', and the sun's shadow map with the bodies' spots on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Targets {
    pub reflection: bool,
    pub ripples: bool,
    pub shadow: bool,
}

/// One frame's share of the readying.
#[derive(Clone, Debug, Default)]
pub struct Share {
    /// Pictures to load.
    pub pictures: Vec<u32>,
    /// Draws whose lists, texture set and programs to make.
    pub draws: Vec<ObjectBatch>,
    /// Shadow casters whose lists and first picture's texture set to make.
    pub casters: Vec<ObjectBatch>,
    /// Pieces of the land whose lists to make.
    pub land: Vec<TerrainBatch>,
    /// The pictures the frame is drawn into, with the first share only.
    pub targets: Option<Targets>,
}

impl Share {
    /// Whether it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
            && self.draws.is_empty()
            && self.casters.is_empty()
            && self.land.is_empty()
            && self.targets.is_none()
    }
}

/// What waits to be readied, each once, in the order it was planned: the pictures first, then the
/// rest.
#[derive(Clone, Debug, Default)]
pub struct Warmup {
    pictures: VecDeque<u32>,
    draws: VecDeque<ObjectBatch>,
    casters: VecDeque<ObjectBatch>,
    land: VecDeque<TerrainBatch>,
    targets: Option<Targets>,
    planned: HashSet<u32>,
    lists: HashSet<DrawKey>,
}

/// At most [`DRAWS_PER_FRAME`] of the first of `q`, taken from it.
fn first<T>(q: &mut VecDeque<T>) -> Vec<T> {
    let n = q.len().min(DRAWS_PER_FRAME);
    q.drain(..n).collect()
}

/// A draw's lists, by their addresses, and its texture set.
type DrawKey = (usize, usize, [(u32, bool); 3]);

/// A draw's lists and texture set, which say what readying it makes.
fn draw_key(b: &ObjectBatch, set: [(u32, bool); 3]) -> DrawKey {
    (
        Arc::as_ptr(&b.vertices) as usize,
        Arc::as_ptr(&b.indices) as usize,
        set,
    )
}

impl Warmup {
    /// Queue `pictures` not queued before (0, no picture, never).
    pub fn plan(&mut self, pictures: impl IntoIterator<Item = u32>) {
        for id in pictures {
            if id != 0 && self.planned.insert(id) {
                self.pictures.push_back(id);
            }
        }
    }

    /// Queue what `frame` will put on the device: its pictures, its draws, its shadow casters, the
    /// land and the pictures it is drawn into.
    pub fn plan_frame(&mut self, frame: &Frame) {
        self.plan(frame.pictures());
        for b in frame.drawn_batches() {
            if !b.indices.is_empty() && self.lists.insert(draw_key(b, texture_set(&b.layer))) {
                self.draws.push_back(b.clone());
            }
        }
        for b in frame.shadow.iter().flat_map(|s| &s.casters) {
            let first = texture_set(&b.layer)[0];
            let key = draw_key(b, [first, (0, false), (0, false)]);
            if !b.indices.is_empty() && self.lists.insert(key) {
                self.casters.push_back(b.clone());
            }
        }
        for t in frame.terrain.iter().chain(&frame.reflection_land) {
            let key = (
                Arc::as_ptr(&t.vertices) as usize,
                Arc::as_ptr(&t.indices) as usize,
                [(0, false); 3],
            );
            if self.lists.insert(key) {
                self.land.push_back(t.clone());
            }
        }
        self.targets = Some(Targets {
            reflection: frame.reflection.is_some(),
            ripples: frame.reflection.is_some_and(|r| r.ripples),
            shadow: frame.shadow.is_some(),
        });
    }

    /// This frame's share: the pictures it is drawn into the first time; at most [`PER_FRAME`] of
    /// the pictures waiting; once none wait, at most [`DRAWS_PER_FRAME`] each of the draws, the
    /// casters and the land.
    pub fn take(&mut self) -> Share {
        let mut share = Share {
            targets: self.targets.take(),
            ..Share::default()
        };
        let n = self.pictures.len().min(PER_FRAME);
        share.pictures = self.pictures.drain(..n).collect();
        if self.pictures.is_empty() && share.pictures.is_empty() {
            share.draws = first(&mut self.draws);
            share.casters = first(&mut self.casters);
            share.land = first(&mut self.land);
        }
        share
    }

    /// Whether anything waits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
            && self.draws.is_empty()
            && self.casters.is_empty()
            && self.land.is_empty()
            && self.targets.is_none()
    }

    /// Forget what was planned and what waits, for the next time.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl Frame {
    /// The draws of the world's objects, each list of them in drawing order: the sky, the
    /// objects, the decals, the shores, the water and the foam.
    pub fn drawn_batches(&self) -> impl Iterator<Item = &ObjectBatch> {
        self.sky_before
            .iter()
            .chain(&self.objects)
            .chain(&self.decals)
            .chain(&self.shore)
            .chain(&self.water)
            .chain(&self.foam)
            .chain(&self.sky_after)
    }

    /// The pictures the frame's draws read, each once, in drawing order: the sky, the land's
    /// ground and blending pictures, the clouds' shadows, the objects, the decals, the shores, the
    /// water and the foam. The lens flares' pictures are not among them.
    #[must_use]
    pub fn pictures(&self) -> Vec<u32> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut add = |id: u32| {
            if id != 0 && seen.insert(id) {
                out.push(id);
            }
        };
        let batches = |list: &[ObjectBatch], add: &mut dyn FnMut(u32)| {
            for b in list {
                for s in bound_stages(&b.layer).into_iter().flatten() {
                    add(s.texture);
                }
            }
        };
        batches(&self.sky_before, &mut add);
        for t in self.terrain.iter().chain(&self.reflection_land) {
            add(t.ground);
            add(t.mask.unwrap_or(0));
        }
        if let Some(c) = &self.clouds {
            add(c.picture);
        }
        for list in [
            &self.objects,
            &self.decals,
            &self.shore,
            &self.water,
            &self.foam,
            &self.sky_after,
        ] {
            batches(list, &mut add);
        }
        out
    }
}
