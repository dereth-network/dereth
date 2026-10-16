//! The far trees, the scene's half: retail's scenery of the trees an art pack has far pictures
//! for, worked out off the frame for every block out to the reach, so Better Lighting draws them
//! to the horizon.
//!
//! Retail's scenery is placed from the land alone: each block's terrain words and heights, the
//! region's scenes and the cells its buildings own. So a block far off grows its trees where it
//! would if it were near, and nothing else of it is loaded. A thread of its own works each
//! block's trees out, nearest first, and the scene keeps them while the block is within reach.
//!
//! A tree stands on the drawn ground's far surface, at the lowest of it or of retail's ground
//! within a few meters, so the far mesh, coarser the farther it lies, leaves no gap beneath it.
//! The presentation leaves out the trees of a block whose own objects stand in the world: those
//! it draws from the block's placements.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, PoisonError};

use dereth_assets::region::Region;
use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;
use dereth_terrain::land::drawn::{self, Knob, Knobs, Point, BLOCK_METERS};
use dereth_terrain::scenery::{generate_scenery, outside_cell_index, SceneryEnv, WithinBlockShape};

use super::drawn_ground::World;
use super::forest_cover::{cover_models, forest_cover, Cover};
use super::hifi_bridge::FarTree;

/// How far round a tree's foot the ground is looked at for the lowest of it, meters.
const FOOT_REACH: f32 = 3.0;
/// The blocks a side of the world.
const WORLD_BLOCKS: i32 = 255;

/// What a block's far trees are worked out from: the same for every block.
struct Sources {
    store: Arc<RetailDatStore>,
    region: Arc<Region>,
    table: [f32; 256],
    world: Arc<World>,
    knobs: Knobs,
    /// The retail objects the art pack has far pictures for.
    ids: Arc<HashSet<u32>>,
}

/// What the thread reads once and keeps: each scene, and each object's shape for the scenery's
/// last test.
#[derive(Default)]
struct Reads {
    scenes: HashMap<DataId, Option<dereth_assets::world::Scene>>,
    shapes: HashMap<u32, Option<WithinBlockShape>>,
}

/// A hash of `v`, mixed so nearby inputs give unrelated outputs.
fn mix(mut v: u32) -> u32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7FEB_352D);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846C_A68B);
    v ^ (v >> 16)
}

/// The lowest of the drawn ground's far surface and retail's ground at world `(x, y)` and
/// [`FOOT_REACH`] round it.
fn foot(world: &World, knobs: &Knobs, x: f32, y: f32) -> f32 {
    let r = FOOT_REACH;
    [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
        .into_iter()
        .map(|(dx, dy)| {
            let p = Point::at(world, x + dx, y + dy);
            p.retail
                .min(p.retail + drawn::far_offset(&p, x + dx, y + dy, knobs))
        })
        .fold(f32::MAX, f32::min)
}

/// Block `(bx, by)`'s trees that the art pack has far pictures for, where retail's scenery
/// places them.
fn block_trees(src: &Sources, reads: &mut Reads, bx: i32, by: i32) -> Vec<FarTree> {
    use dereth_client_runtime::world_build::{read_landblock, read_lbi, read_scene};
    let Some(lb) = read_landblock(&src.store, bx, by) else {
        return Vec::new();
    };
    let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
        &lb,
        &src.region,
        &src.table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    // A cell that owns a building grows no scenery.
    let mut owned = [false; 65];
    if let Some(info) = read_lbi(&src.store, bx, by) {
        for b in &info.buildings {
            owned[usize::from(outside_cell_index(b.frame.origin.x, b.frame.origin.y))] = true;
        }
    }
    let store = &src.store;
    let reads = std::cell::RefCell::new(reads);
    let env = SceneryEnv {
        scenes: &|id| {
            reads
                .borrow_mut()
                .scenes
                .entry(id)
                .or_insert_with(|| read_scene(store, id))
                .clone()
        },
        has_building: &|cell| owned.get(usize::from(cell)).copied().unwrap_or(false),
        shape: &|id| {
            reads
                .borrow_mut()
                .shapes
                .entry(id.0)
                .or_insert_with(|| dereth_client_runtime::models::within_block_shape(store, id))
                .clone()
        },
    };
    let placed = generate_scenery(&lb, &mesh, &src.region, bx, by, &env);
    // LINT-OK: block coordinates, far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (bx as f32 * BLOCK_METERS, by as f32 * BLOCK_METERS);
    placed
        .iter()
        .filter(|p| src.ids.contains(&p.gfxobj.0))
        .map(|p| {
            let (x, y) = (ox + p.frame.origin.x, oy + p.frame.origin.y);
            let q = p.frame.rotation;
            FarTree {
                at: glam::Vec3::new(x, y, foot(&src.world, &src.knobs, x, y)),
                rotation: glam::Quat::from_xyzw(q.x, q.y, q.z, q.w),
                scale: p.scale,
                id: p.gfxobj.0,
                seed: mix(x.to_bits() ^ mix(y.to_bits()) ^ p.gfxobj.0),
            }
        })
        .collect()
}

/// The blocks still to work out, nearest first, and the signal that more are asked for.
type Queue = Arc<(Mutex<VecDeque<(i32, i32)>>, Condvar)>;

/// The thread that works the blocks out, and what it shares with the frame.
struct Worker {
    /// The blocks still to work out, nearest first; the frame replaces them as the eye moves.
    queue: Queue,
    /// Each block worked out, as it is done, with the milliseconds it took.
    done: mpsc::Receiver<((i32, i32), Vec<FarTree>, f64)>,
    /// Set to end the thread.
    stop: Arc<AtomicBool>,
}

impl Worker {
    fn start(src: Sources) -> Option<Self> {
        let queue: Queue = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (send, done) = mpsc::channel();
        let (q, halt) = (Arc::clone(&queue), Arc::clone(&stop));
        std::thread::Builder::new()
            .name("far trees".to_owned())
            .spawn(move || {
                let mut reads = Reads::default();
                loop {
                    let next = {
                        let (lock, wake) = &*q;
                        let mut list = lock.lock().unwrap_or_else(PoisonError::into_inner);
                        while list.is_empty() && !halt.load(Ordering::Relaxed) {
                            list = wake.wait(list).unwrap_or_else(PoisonError::into_inner);
                        }
                        list.pop_front()
                    };
                    if halt.load(Ordering::Relaxed) {
                        return;
                    }
                    let Some((bx, by)) = next else {
                        continue;
                    };
                    let started = web_time::Instant::now();
                    let trees = block_trees(&src, &mut reads, bx, by);
                    let ms = started.elapsed().as_secs_f64() * 1000.0;
                    if send.send(((bx, by), trees, ms)).is_err() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self { queue, done, stop })
    }

    /// Work out `blocks` next, in their order, in place of whatever was still to do.
    fn ask(&self, blocks: impl IntoIterator<Item = (i32, i32)>) {
        let (lock, wake) = &*self.queue;
        let mut list = lock.lock().unwrap_or_else(PoisonError::into_inner);
        list.clear();
        list.extend(blocks);
        wake.notify_one();
    }

    /// Whether blocks are still to be worked out.
    fn busy(&self) -> bool {
        let (lock, _) = &*self.queue;
        !lock
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

/// What the far trees were worked out for: a change of any throws them away.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    /// The art pack's serial.
    pack: u64,
    /// The world's vertices' serial.
    world: u64,
    /// The knobs the far surface is made from.
    surface: [u32; 3],
}

/// The far trees' state in the scene.
#[derive(Default)]
pub(crate) struct FarTrees {
    /// The forest cover, once worked out, with its serial and the pack and world it was worked
    /// out for.
    pub(crate) cover: Option<(u64, Arc<Cover>)>,
    cover_for: Option<(u64, u64)>,
    cover_loading: Option<mpsc::Receiver<Cover>>,
    /// How many covers have been worked out: the next one's serial.
    covers: u64,
    worker: Option<Worker>,
    /// Each block's trees, once worked out.
    made: HashMap<(i32, i32), Arc<Vec<FarTree>>>,
    /// The blocks asked for and not yet done.
    asked: HashSet<(i32, i32)>,
    key: Option<Key>,
    /// The blocks within reach, nearest first, and the eye block and reach they were worked out
    /// for.
    wanted: Vec<(i32, i32)>,
    wanted_for: Option<((i32, i32), u32)>,
    /// Bumped whenever the trees within reach change.
    pub(crate) serial: u64,
    /// Whether the far trees are drawn this frame.
    pub(crate) active: bool,
    /// How many blocks have been worked out, the milliseconds the thread spent on them, and
    /// the milliseconds the frames spent keeping them.
    pub(crate) taken: (u64, f64, f64),
}

impl std::fmt::Debug for FarTrees {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FarTrees")
            .field("made", &self.made.len())
            .field("asked", &self.asked.len())
            .field("serial", &self.serial)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

/// The blocks whose nearest point lies within `reach` meters of world `eye`, nearest first.
#[must_use]
pub(crate) fn blocks_within(eye: (f32, f32), reach: f32) -> Vec<(i32, i32)> {
    let span = dereth_primitives::num::floor_to_i32(reach / BLOCK_METERS) + 1;
    let (ex, ey) = (
        dereth_primitives::num::floor_to_i32(eye.0 / BLOCK_METERS),
        dereth_primitives::num::floor_to_i32(eye.1 / BLOCK_METERS),
    );
    let mut out: Vec<((i32, i32), f32)> = Vec::new();
    for bx in (ex - span).max(0)..=(ex + span).min(WORLD_BLOCKS - 1) {
        for by in (ey - span).max(0)..=(ey + span).min(WORLD_BLOCKS - 1) {
            // LINT-OK: block coordinates, far inside f32's exact range.
            #[allow(clippy::cast_precision_loss)]
            let (lo_x, lo_y) = (bx as f32 * BLOCK_METERS, by as f32 * BLOCK_METERS);
            let dx = (lo_x - eye.0).max(0.0).max(eye.0 - (lo_x + BLOCK_METERS));
            let dy = (lo_y - eye.1).max(0.0).max(eye.1 - (lo_y + BLOCK_METERS));
            let d = (dx * dx + dy * dy).sqrt();
            if d <= reach {
                out.push(((bx, by), d));
            }
        }
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    out.into_iter().map(|(b, _)| b).collect()
}

impl FarTrees {
    /// One frame's upkeep. `wanted` when the far trees are drawn at all; the art pack `pack` (its
    /// serial and the retail objects it has far pictures for), the world's vertices and their
    /// serial, the knobs, and the eye in world meters.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn step(
        &mut self,
        wanted: bool,
        pack: Option<&Arc<crate::art_pack::ArtPack>>,
        world: Option<(&Arc<World>, u64)>,
        knobs: &Knobs,
        store: &Arc<RetailDatStore>,
        region: &dyn Fn() -> Arc<Region>,
        table: &[f32; 256],
        eye: (f32, f32),
    ) {
        let (Some(pack), Some((world, world_serial)), true) = (pack, world, wanted) else {
            if self.active || self.worker.is_some() || self.cover.is_some() {
                *self = Self {
                    serial: self.serial + 1,
                    covers: self.covers,
                    ..Self::default()
                };
            }
            return;
        };
        self.step_cover(pack, world, world_serial, store, region);
        if !knobs.on(Knob::FarTrees) {
            if self.active || self.worker.is_some() {
                self.worker = None;
                self.made.clear();
                self.asked.clear();
                self.key = None;
                self.wanted_for = None;
                self.active = false;
                self.serial += 1;
            }
            return;
        }
        self.active = true;
        let key = Key {
            pack: pack.serial,
            world: world_serial,
            surface: [Knob::FarRound, Knob::FarDetail, Knob::FarDetailScale]
                .map(|k| knobs.get(k).to_bits()),
        };
        if self.key.as_ref() != Some(&key) || self.worker.is_none() {
            let ids: HashSet<u32> = pack
                .models
                .iter()
                .filter(|m| m.lods.iter().any(|l| l.views.is_some()))
                .map(|m| m.id.0)
                .collect();
            self.made.clear();
            self.asked.clear();
            self.wanted_for = None;
            self.serial += 1;
            self.worker = if ids.is_empty() {
                None
            } else {
                Worker::start(Sources {
                    store: Arc::clone(store),
                    region: region(),
                    table: *table,
                    world: Arc::clone(world),
                    knobs: knobs.clone(),
                    ids: Arc::new(ids),
                })
            };
            self.key = Some(key);
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let started = web_time::Instant::now();
        let mut changed = false;
        while let Ok((block, trees, ms)) = worker.done.try_recv() {
            self.asked.remove(&block);
            self.made.insert(block, Arc::new(trees));
            self.taken.0 += 1;
            self.taken.1 += ms;
            changed = true;
        }
        let reach = knobs.get(Knob::TreeReach);
        let eye_block = (
            dereth_primitives::num::floor_to_i32(eye.0 / BLOCK_METERS),
            dereth_primitives::num::floor_to_i32(eye.1 / BLOCK_METERS),
        );
        if self.wanted_for != Some((eye_block, reach.to_bits())) {
            // Out to the reach from anywhere in the eye's block, so a step within it asks for
            // nothing new.
            // LINT-OK: block coordinates, far inside f32's exact range.
            #[allow(clippy::cast_precision_loss)]
            let centre = (
                (eye_block.0 as f32 + 0.5) * BLOCK_METERS,
                (eye_block.1 as f32 + 0.5) * BLOCK_METERS,
            );
            self.wanted = blocks_within(centre, reach + BLOCK_METERS * 0.75);
            self.wanted_for = Some((eye_block, reach.to_bits()));
            // Kept a ring past the reach, so walking back and forth over a block's edge works
            // nothing out again.
            let margin: HashSet<(i32, i32)> = blocks_within(centre, reach + BLOCK_METERS * 2.5)
                .into_iter()
                .collect();
            self.made.retain(|b, _| margin.contains(b));
            let todo: Vec<(i32, i32)> = self
                .wanted
                .iter()
                .copied()
                .filter(|b| !self.made.contains_key(b))
                .collect();
            self.asked = todo.iter().copied().collect();
            worker.ask(todo);
            changed = true;
        }
        if changed {
            self.serial += 1;
        }
        self.taken.2 += started.elapsed().as_secs_f64() * 1000.0;
    }

    /// The forest cover's upkeep: worked out on a thread of its own once for each pack and
    /// world, and taken when it is done.
    fn step_cover(
        &mut self,
        pack: &Arc<crate::art_pack::ArtPack>,
        world: &Arc<World>,
        world_serial: u64,
        store: &Arc<RetailDatStore>,
        region: &dyn Fn() -> Arc<Region>,
    ) {
        let key = (pack.serial, world_serial);
        if self.cover_for != Some(key) {
            self.cover_for = Some(key);
            self.cover = None;
            let (send, receive) = mpsc::channel();
            let (pack, world, store, region) = (
                Arc::clone(pack),
                Arc::clone(world),
                Arc::clone(store),
                region(),
            );
            let spawned = std::thread::Builder::new()
                .name("forest cover".to_owned())
                .spawn(move || {
                    let models = cover_models(&pack);
                    let mut scenes =
                        |id| dereth_client_runtime::world_build::read_scene(&store, id);
                    let _ = send.send(forest_cover(&world, &region, &mut scenes, &models));
                });
            self.cover_loading = spawned.ok().map(|_| receive);
        }
        if let Some(cover) = self.cover_loading.as_ref().and_then(|r| r.try_recv().ok()) {
            self.covers += 1;
            self.cover = Some((self.covers, Arc::new(cover)));
            self.cover_loading = None;
        }
    }

    /// The trees of the blocks within reach that are worked out, block by block, nearest first.
    pub(crate) fn within(&self) -> impl Iterator<Item = ((i32, i32), &Arc<Vec<FarTree>>)> {
        self.wanted
            .iter()
            .filter_map(|b| self.made.get(b).map(|t| (*b, t)))
    }

    /// Wait until every block within reach is worked out: for a capture that draws them from
    /// its first frames.
    pub(crate) fn wait(&mut self) {
        if let Some(cover) = self.cover_loading.take().and_then(|r| r.recv().ok()) {
            self.covers += 1;
            self.cover = Some((self.covers, Arc::new(cover)));
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let started = web_time::Instant::now();
        while !self.asked.is_empty() || worker.busy() {
            match worker.done.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok((block, trees, ms)) => {
                    self.asked.remove(&block);
                    self.made.insert(block, Arc::new(trees));
                    self.taken.0 += 1;
                    self.taken.1 += ms;
                }
                Err(_) => break,
            }
            if started.elapsed() > std::time::Duration::from_secs(300) {
                break;
            }
        }
        self.serial += 1;
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    #[test]
    fn the_blocks_within_reach_are_every_block_whose_nearest_point_is_that_near_nearest_first() {
        // The eye in the middle of block (100, 100).
        let eye = (100.5 * BLOCK_METERS, 100.5 * BLOCK_METERS);
        let blocks = blocks_within(eye, 500.0);
        assert_eq!(blocks[0], (100, 100), "the eye's own block first");
        // Its neighbours touch it, at 96 m from the middle; two blocks over, 288 m; three, 480.
        assert!(blocks.contains(&(103, 100)));
        assert!(!blocks.contains(&(104, 100)), "{blocks:?}");
        // A corner block three over each way lies 480 m off on each axis: out of reach.
        assert!(!blocks.contains(&(103, 103)));
        // Nearest first: every block a step further out comes after the eye's ring of
        // neighbours.
        let ring = |b: &(i32, i32)| (b.0 - 100).abs().max((b.1 - 100).abs());
        let first_far = blocks
            .iter()
            .position(|b| ring(b) == 2)
            .expect("a block two over");
        assert!(
            blocks[..first_far].iter().all(|b| ring(b) <= 1),
            "{blocks:?}"
        );
        // The world's edge bounds them.
        let corner = blocks_within((10.0, 10.0), 1000.0);
        assert!(corner.iter().all(|(x, y)| *x >= 0 && *y >= 0));
    }

    #[test]
    fn a_trees_seed_is_the_same_whenever_it_is_worked_out_and_its_neighbours_differ() {
        let a = mix(100.25f32.to_bits() ^ mix(200.5f32.to_bits()) ^ 0x0200_0258);
        let b = mix(100.25f32.to_bits() ^ mix(200.5f32.to_bits()) ^ 0x0200_0258);
        let c = mix(100.5f32.to_bits() ^ mix(200.5f32.to_bits()) ^ 0x0200_0258);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
