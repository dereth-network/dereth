//! The drawn ground, the scene's half: while the high-fidelity presentation draws an art pack's
//! ground in the better lighting, it draws the landscape itself, retail's surface with a visual
//! offset (see [`dereth_terrain::land::drawn`]), and the scene draws everything that stands on the
//! ground moved by the same offset: bodies, items, scenery and the camera.
//!
//! Collision, physics, line of sight, picking and the server never see the offset. What moves is
//! where things are drawn: a server object's parts, the player's body and the static scenery as
//! it is baked. Buildings, static objects, roads and water pin the offset to nought, so they and
//! their rooms stand where they always did.
//!
//! Here the scene keeps:
//! - **the knobs** (`terrain-tuning.toml`, read again whenever it changes while the client
//!   runs);
//! - **the world's landscape vertices**, read once, on a thread of their own, when the drawn
//!   ground is first wanted, with where the water lies about them; the presentation draws the
//!   far ground and the water from them;
//! - **each near block's offset**, worked out when it is first needed and thrown away when a
//!   knob it is made from changes;
//! - **the pins**: each building's and static object's footprint (the box of its geometry within
//!   2.5 meters of its base, turned with it) and its margin.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use dereth_assets::Decode as _;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame, Vec3};
use dereth_terrain::land::drawn::{
    self, GroundSource, Knob, Knobs, Pin, Point, Vertex, BLOCK_METERS,
};

/// The world's landscape vertices a side.
pub(crate) const WORLD_SIDE: usize = 2041;
/// The height above a model's base its footprint is taken from: what meets the ground.
const FOOT: f32 = 2.5;
/// How many frames pass between looks at the tuning file.
const TUNING_EVERY: u32 = 15;
/// How far above a body's feet the camera that follows it pivots, meters: about its eyes.
const CAMERA_PIVOT: f32 = 1.6;
/// How far apart the camera's line from that pivot is tried against the drawn ground, meters.
const BOOM_STEP: f32 = 0.25;
/// The most blocks whose offset is worked out in one frame, past those a bake needs at once.
const FIELDS_A_FRAME: usize = 6;

/// The knobs a block's offset is made from: a change to any throws the offsets away.
const FIELD_KNOBS: [Knob; 18] = [
    Knob::Round,
    Knob::WalkCap,
    Knob::PinFade,
    Knob::PinBuildingMargin,
    Knob::PinStaticMargin,
    Knob::PinRoads,
    Knob::PinWater,
    Knob::PinWaterLevel,
    Knob::Cliffs,
    Knob::CliffCap,
    Knob::CliffCapTall,
    Knob::CliffTallRelief,
    Knob::CliffEdge,
    Knob::CliffGrowth,
    Knob::CliffBand,
    Knob::CliffRiser,
    Knob::CliffNoise,
    Knob::CliffNoiseScale,
];

/// The knobs that move what is baked at a block's offset: the scenery is baked again when one
/// changes.
const BAKE_KNOBS: [Knob; 5] = [
    Knob::Ground,
    Knob::BodyShift,
    Knob::WalkLowpass,
    Knob::ShiftFade,
    Knob::ShiftGone,
];

/// The world's landscape vertices, `WORLD_SIDE` a side, row by row from the south.
#[derive(Debug)]
pub(crate) struct World {
    pub(crate) heights: Arc<Vec<f32>>,
    pub(crate) words: Arc<Vec<u16>>,
    /// Where the water lies about each vertex and its level, as the presentation holds it (see
    /// [`drawn::shore`]).
    pub(crate) water: Arc<Vec<[f32; 2]>>,
    /// How long they took to read, milliseconds.
    pub(crate) read_ms: f64,
}

impl GroundSource for World {
    fn vertex(&self, gx: i32, gy: i32) -> Option<Vertex> {
        let side = i32::try_from(WORLD_SIDE).ok()?;
        if !(0..side).contains(&gx) || !(0..side).contains(&gy) {
            return None;
        }
        let k = usize::try_from(gy).ok()? * WORLD_SIDE + usize::try_from(gx).ok()?;
        Some(Vertex {
            height: self.heights[k],
            word: self.words[k],
        })
    }
}

/// Every landblock of the cell file, its heights through `table`: deep sea where the file has
/// none; and where the water lies about them.
pub(crate) fn read_world(store: &RetailDatStore, table: &[f32; 256]) -> World {
    let started = web_time::Instant::now();
    let mut heights = vec![0.0f32; WORLD_SIDE * WORLD_SIDE];
    let mut words = vec![20u16 << 2; WORLD_SIDE * WORLD_SIDE];
    for bx in 0..255usize {
        for by in 0..255usize {
            // LINT-OK: block coordinates, 0..255.
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let Some(lb) =
                dereth_client_runtime::world_build::read_landblock(store, bx as i32, by as i32)
            else {
                continue;
            };
            for i in 0..9 {
                for j in 0..9 {
                    let k = (by * 8 + j) * WORLD_SIDE + bx * 8 + i;
                    heights[k] = table[usize::from(lb.height[i * 9 + j])];
                    words[k] = lb.terrain[i * 9 + j];
                }
            }
        }
    }
    let water = drawn::shore::held(&drawn::shore::water_field(WORLD_SIDE, &heights, &words));
    World {
        heights: Arc::new(heights),
        words: Arc::new(words),
        water: Arc::new(water),
        read_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}

/// A model's footprint in its own space: the box, east and north, of its vertices within
/// [`FOOT`] of its base; `None` for a model with none there.
fn footprint(store: &RetailDatStore, id: DataId) -> Option<[f32; 4]> {
    let gfxobj = |id: DataId| -> Option<dereth_assets::GfxObj> {
        let bytes = store.read_typed(DbType::GfxObj, id).ok()?;
        dereth_assets::GfxObj::decode_payload_in(store.era_of(id), id, &bytes).ok()
    };
    let mut boxed: Option<[f32; 4]> = None;
    let mut take = |p: Vec3| {
        if p.z > FOOT {
            return;
        }
        let b = boxed.get_or_insert([p.x, p.y, p.x, p.y]);
        b[0] = b[0].min(p.x);
        b[1] = b[1].min(p.y);
        b[2] = b[2].max(p.x);
        b[3] = b[3].max(p.y);
    };
    match id.0 >> 24 {
        0x01 => {
            for v in &gfxobj(id)?.vertex_array.vertices {
                take(v.position);
            }
        }
        0x02 => {
            let bytes = store.read_typed(DbType::Setup, id).ok()?;
            let setup =
                dereth_assets::Setup::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
            let rest = setup
                .placement_frames
                .get(&0x65)
                .or_else(|| setup.placement_frames.get(&0))
                .map(|p| p.frames.clone())
                .unwrap_or_default();
            for (k, part) in setup.parts.iter().enumerate() {
                let at = rest.get(k).copied().unwrap_or_default();
                let Some(g) = gfxobj(*part) else {
                    continue;
                };
                for v in &g.vertex_array.vertices {
                    take(dereth_primitives::frame::localtoglobal(&at, v.position));
                }
            }
        }
        _ => return None,
    }
    boxed
}

/// The footprint `fp` of an object placed at `frame` in block `(bx, by)`, with `margin` round it,
/// as global ground kept at retail's height.
fn pin_of(fp: [f32; 4], frame: &Frame, (bx, by): (i32, i32), margin: f32) -> Pin {
    let middle = Vec3::new((fp[0] + fp[2]) * 0.5, (fp[1] + fp[3]) * 0.5, 0.0);
    let at = dereth_primitives::frame::localtoglobal(frame, middle);
    let east = dereth_primitives::frame::localtoglobal(
        &Frame::new(Vec3::ZERO, frame.rotation),
        Vec3::new(1.0, 0.0, 0.0),
    );
    let len = (east.x * east.x + east.y * east.y).sqrt();
    let axis = if len > 1e-4 {
        (east.x / len, east.y / len)
    } else {
        (1.0, 0.0)
    };
    // LINT-OK: block coordinates, far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (bx as f32 * BLOCK_METERS, by as f32 * BLOCK_METERS);
    Pin {
        x: ox + at.x,
        y: oy + at.y,
        half: ((fp[2] - fp[0]) * 0.5, (fp[3] - fp[1]) * 0.5),
        axis,
        margin,
    }
}

/// One block's near offset as it is kept and handed to the presentation.
#[derive(Debug, Clone)]
pub(crate) struct Tile {
    /// A number of its own, so the presentation sees a tile worked out again.
    pub(crate) generation: u64,
    /// The offset bodies stand on, [`drawn::SIDE`] a side.
    pub(crate) offset: Arc<Vec<f32>>,
    /// How much of it the pins leave.
    pub(crate) weight: Arc<Vec<f32>>,
}

/// The tuning file: where it is, and what it held when it was last read.
#[derive(Debug, Default)]
struct Tuning {
    path: Option<PathBuf>,
    text: Option<String>,
    frames: u32,
}

/// The drawn ground's state in the scene.
#[derive(Debug, Default)]
pub(crate) struct DrawnGround {
    pub(crate) knobs: Knobs,
    world: Option<Arc<World>>,
    world_serial: u64,
    loading: Option<mpsc::Receiver<World>>,
    fields: HashMap<(i32, i32), Tile>,
    /// The number the next tile takes.
    next_tile: u64,
    /// How many offsets have been worked out, and the milliseconds they took together.
    worked: (u64, f64),
    pins: HashMap<(i32, i32), Arc<Vec<Pin>>>,
    footprints: HashMap<DataId, Option<[f32; 4]>>,
    /// Bumped whenever the offsets are thrown away for new knobs.
    pub(crate) field_generation: u64,
    /// Bumped whenever what is baked at the offset changes: the offsets, or whether the drawn
    /// ground is drawn, or a knob that moves the scenery.
    pub(crate) bake_generation: u64,
    tuning: Tuning,
    /// Whether the drawn ground is drawn this frame.
    pub(crate) active: bool,
    /// Where render space's origin lies in the world, meters east and north.
    pub(crate) global: (f32, f32),
    /// The viewer's block.
    pub(crate) viewer: (i32, i32),
    /// The eye, render space.
    pub(crate) eye: Vec3,
}

impl DrawnGround {
    /// Read the knobs from `path` from now on, writing it with every knob at its default when
    /// there is no such file; `None` keeps the knobs as they are.
    pub(crate) fn set_tuning_file(&mut self, path: Option<PathBuf>) {
        self.tuning = Tuning {
            path,
            text: None,
            frames: TUNING_EVERY,
        };
    }

    /// The tuning file, where one is set.
    pub(crate) fn tuning_file(&self) -> Option<&std::path::Path> {
        self.tuning.path.as_deref()
    }

    /// Set the knobs, as a change of the tuning file would.
    pub(crate) fn set_knobs(&mut self, knobs: Knobs) {
        let changed = knobs.changed_from(&self.knobs);
        self.knobs = knobs;
        if changed.iter().any(|k| FIELD_KNOBS.contains(k)) {
            self.fields.clear();
            self.pins.clear();
            self.field_generation += 1;
            self.bake_generation += 1;
        } else if changed.iter().any(|k| BAKE_KNOBS.contains(k)) {
            self.bake_generation += 1;
        }
    }

    /// Look at the tuning file every few frames, and take its knobs when what it holds has
    /// changed. A file that is not there is written with every knob at its default.
    fn poll_tuning(&mut self) {
        self.tuning.frames += 1;
        if self.tuning.frames < TUNING_EVERY {
            return;
        }
        self.tuning.frames = 0;
        let Some(path) = self.tuning.path.clone() else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let defaults = Knobs::default().to_text();
                if let Err(e) = std::fs::write(&path, &defaults) {
                    tracing::warn!(
                        "the terrain tuning file {} could not be written: {e}",
                        path.display()
                    );
                }
                self.tuning.text = Some(defaults);
                return;
            }
            Err(_) => return,
        };
        if self.tuning.text.as_deref() == Some(text.as_str()) {
            return;
        }
        let (knobs, notes) = Knobs::parse(&text);
        self.tuning.text = Some(text);
        for n in notes {
            tracing::warn!("{}: {n}", path.display());
        }
        let changed = knobs.changed_from(&self.knobs);
        if !changed.is_empty() {
            tracing::info!(
                "terrain tuning: {} changed",
                changed
                    .iter()
                    .map(|k| k.spec().name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        self.set_knobs(knobs);
    }

    /// One frame's upkeep: the tuning file, the world's vertices (asked for from `store` when the
    /// drawn ground is `wanted` and they have not been), and whether the drawn ground is drawn.
    pub(crate) fn step(&mut self, wanted: bool, store: &Arc<RetailDatStore>, table: &[f32; 256]) {
        if wanted {
            self.poll_tuning();
        }
        let wanted = wanted && self.knobs.on(Knob::Ground);
        if wanted && self.world.is_none() && self.loading.is_none() {
            let (send, receive) = mpsc::channel();
            let store = Arc::clone(store);
            let table = *table;
            let spawned = crate::workers::spawn("landscape vertices", move || {
                let _ = send.send(read_world(&store, &table));
            });
            if spawned.is_ok() {
                self.loading = Some(receive);
            }
        }
        if let Some(world) = self.loading.as_ref().and_then(|r| r.try_recv().ok()) {
            self.world = Some(Arc::new(world));
            self.world_serial += 1;
            self.loading = None;
        }
        let active = wanted && self.world.is_some();
        if active != self.active {
            self.active = active;
            self.bake_generation += 1;
        }
    }

    /// Wait for the world's vertices, where they are being read, and take them: for a capture
    /// that has to draw the ground from its first frame.
    pub(crate) fn wait_for_world(&mut self) {
        if let Some(world) = self.loading.take().and_then(|r| r.recv().ok()) {
            self.world = Some(Arc::new(world));
            self.world_serial += 1;
        }
    }

    /// The world's vertices and their serial, once read.
    pub(crate) fn world(&self) -> Option<(&Arc<World>, u64)> {
        self.world.as_ref().map(|w| (w, self.world_serial))
    }

    /// The pins standing on block `block` or near enough to reach it, worked out once.
    fn pins_for(&mut self, store: &RetailDatStore, block: (i32, i32)) -> Arc<Vec<Pin>> {
        if let Some(p) = self.pins.get(&block) {
            return Arc::clone(p);
        }
        let reach = self.knobs.get(Knob::PinFade) + 48.0;
        // LINT-OK: block coordinates, far inside f32's exact range.
        #[allow(clippy::cast_precision_loss)]
        let (lo, hi) = (
            (
                block.0 as f32 * BLOCK_METERS - reach,
                block.1 as f32 * BLOCK_METERS - reach,
            ),
            (
                (block.0 + 1) as f32 * BLOCK_METERS + reach,
                (block.1 + 1) as f32 * BLOCK_METERS + reach,
            ),
        );
        let mut pins = Vec::new();
        for nx in block.0 - 1..=block.0 + 1 {
            for ny in block.1 - 1..=block.1 + 1 {
                let Some(info) = dereth_client_runtime::world_build::read_lbi(store, nx, ny) else {
                    continue;
                };
                let objects = info
                    .buildings
                    .iter()
                    .map(|b| (b.id, b.frame, self.knobs.get(Knob::PinBuildingMargin)))
                    .chain(
                        info.objects
                            .iter()
                            .map(|o| (o.id, o.frame, self.knobs.get(Knob::PinStaticMargin))),
                    )
                    .collect::<Vec<_>>();
                for (id, frame, margin) in objects {
                    let fp = *self
                        .footprints
                        .entry(id)
                        .or_insert_with(|| footprint(store, id));
                    let Some(fp) = fp else {
                        continue;
                    };
                    let pin = pin_of(fp, &frame, (nx, ny), margin);
                    let r = pin.reach();
                    if pin.x + r >= lo.0
                        && pin.x - r <= hi.0
                        && pin.y + r >= lo.1
                        && pin.y - r <= hi.1
                    {
                        pins.push(pin);
                    }
                }
            }
        }
        let pins = Arc::new(pins);
        self.pins.insert(block, Arc::clone(&pins));
        pins
    }

    /// Block `block`'s offset, worked out now if it has not been; `None` before the world's
    /// vertices are read.
    pub(crate) fn field(&mut self, store: &RetailDatStore, block: (i32, i32)) -> Option<Tile> {
        if let Some(t) = self.fields.get(&block) {
            return Some(t.clone());
        }
        let world = Arc::clone(self.world.as_ref()?);
        let started = web_time::Instant::now();
        let pins = self.pins_for(store, block);
        let field = drawn::block_field(world.as_ref(), block, &pins, &self.knobs);
        self.worked.0 += 1;
        self.worked.1 += started.elapsed().as_secs_f64() * 1000.0;
        self.next_tile += 1;
        let tile = Tile {
            generation: self.next_tile,
            offset: Arc::new(field.offset),
            weight: Arc::new(field.weight),
        };
        self.fields.insert(block, tile.clone());
        Some(tile)
    }

    /// Work out the offsets of the blocks round the viewer's that lack one, a few a frame, and
    /// forget those far from it.
    pub(crate) fn keep_fields(&mut self, store: &RetailDatStore) {
        if !self.active {
            return;
        }
        // LINT-OK: a knob of 1 to 6.
        #[allow(clippy::cast_possible_truncation)]
        let reach = self.knobs.get(Knob::NearBlocks).round() as i32;
        let (vx, vy) = self.viewer;
        self.fields
            .retain(|(x, y), _| (x - vx).abs() <= reach + 1 && (y - vy).abs() <= reach + 1);
        self.pins
            .retain(|(x, y), _| (x - vx).abs() <= reach + 2 && (y - vy).abs() <= reach + 2);
        let mut wanted: Vec<(i32, i32)> = (vx - reach..=vx + reach)
            .flat_map(|x| (vy - reach..=vy + reach).map(move |y| (x, y)))
            .filter(|b| (0..255).contains(&b.0) && (0..255).contains(&b.1))
            .filter(|b| !self.fields.contains_key(b))
            .collect();
        wanted.sort_by_key(|(x, y)| (x - vx).abs().max((y - vy).abs()));
        for b in wanted.into_iter().take(FIELDS_A_FRAME) {
            let _ = self.field(store, b);
        }
    }

    /// The offsets worked out, by block.
    pub(crate) fn tiles(&self) -> &HashMap<(i32, i32), Tile> {
        &self.fields
    }

    /// The near offset at global `(x, y)`, from the block's offset where it has one; nought
    /// where none is worked out yet.
    fn near_at(&self, x: f32, y: f32) -> f32 {
        near_at(|b| self.fields.get(&b).map(|t| t.offset.as_slice()), x, y)
    }

    /// The water as the drawn ground lays it, while it is drawn as water.
    fn water(&self) -> Option<drawn::Water<'_>> {
        water(self.world.as_deref()?, &self.knobs)
    }

    /// How much of the shift a thing `above` meters over retail's ground keeps.
    fn keep(&self, above: f32) -> f32 {
        keep(&self.knobs, above)
    }

    /// How far a thing standing at render-space `at` is drawn above where physics puts it: the
    /// offset averaged over its stance, given way to the far ground with its distance from the
    /// eye, and faded out as it stands higher over retail's ground; and never under the drawn
    /// ground there, the water's bed and the floor of its banks included. Nought while the drawn
    /// ground is not drawn, or bodies are not drawn on it.
    pub(crate) fn shift_at(&self, at: Vec3) -> f32 {
        if !self.active || !self.knobs.on(Knob::BodyShift) {
            return 0.0;
        }
        let Some(world) = self.world.as_ref() else {
            return 0.0;
        };
        let (x, y) = (at.x + self.global.0, at.y + self.global.1);
        let p = Point::at(world.as_ref(), x, y);
        let keep = self.keep(at.z - p.retail);
        if keep <= 0.0 {
            return 0.0;
        }
        let eye = self.eye;
        let dist =
            ((at.x - eye.x).powi(2) + (at.y - eye.y).powi(2) + (p.retail - eye.z).powi(2)).sqrt();
        let far = drawn::morph(dist, &self.knobs);
        let near = if far < 1.0 {
            drawn::body_offset(|x, y| self.near_at(x, y), x, y, &self.knobs)
        } else {
            0.0
        };
        let ground = drawn::drawn_at(&p, x, y, near, far, self.water().as_ref(), &self.knobs);
        drawn::shift(at.z, ground, keep)
    }

    /// What block `block`'s scenery is baked moved by, its offset worked out first if it has
    /// not been: `None` while the drawn ground is not drawn. Taken when the block's bake is
    /// asked for, so a bake off the frame moves the scenery as one on the frame does.
    pub(crate) fn scenery_shift_for(
        &mut self,
        store: &RetailDatStore,
        block: (i32, i32),
    ) -> Option<SceneryShift> {
        if !self.active {
            return None;
        }
        self.field(store, block)?;
        self.scenery_shift_held(block)
    }

    /// What block `block`'s scenery is baked moved by, from the offsets worked out so far:
    /// `None` until the block's own is.
    fn scenery_shift_held(&self, block: (i32, i32)) -> Option<SceneryShift> {
        if !self.fields.contains_key(&block) {
            return None;
        }
        let world = Arc::clone(self.world.as_ref()?);
        let tiles = (block.0 - 1..=block.0 + 1)
            .flat_map(|x| (block.1 - 1..=block.1 + 1).map(move |y| (x, y)))
            .filter_map(|b| self.fields.get(&b).map(|t| (b, Arc::clone(&t.offset))))
            .collect();
        Some(SceneryShift {
            block,
            world,
            knobs: self.knobs.clone(),
            tiles,
        })
    }

    /// How far the camera at render-space `eye` is drawn from where it stands, `body` the feet
    /// (render space, where physics puts them) of the body it follows, if any.
    ///
    /// - **Its rise** is the body's own: the camera is drawn moved as the body it follows is, so
    ///   it rides with the body over the drawn ground and glides where physics steps over
    ///   retail's facets. A camera that follows no body rises by the offset under it.
    /// - **Its line** from the pivot over the body's feet to the camera is tried against the
    ///   drawn ground, and where the ground rises into it the camera is drawn in front of the
    ///   rise, as the camera's own sweep draws it in front of the ground physics holds.
    /// - **Its clearance:** it is never drawn closer to the drawn ground than the camera's
    ///   clearance.
    pub(crate) fn camera_offset(&self, eye: Vec3, body: Option<Vec3>) -> Vec3 {
        if !self.active || !self.knobs.on(Knob::BodyShift) || self.world.is_none() {
            return Vec3::ZERO;
        }
        let clearance = self.knobs.get(Knob::CameraClearance) + self.knobs.get(Knob::Micro);
        let rise = match body {
            Some(b) => self.shift_at(b),
            None => {
                let (x, y) = (eye.x + self.global.0, eye.y + self.global.1);
                drawn::body_offset(|x, y| self.near_at(x, y), x, y, &self.knobs)
            }
        };
        let mut at = Vec3::new(eye.x, eye.y, eye.z + rise);
        if let Some(b) = body {
            let pivot = Vec3::new(b.x, b.y, b.z + rise + CAMERA_PIVOT);
            let boom = Vec3::new(at.x - pivot.x, at.y - pivot.y, at.z - pivot.z);
            let length = (boom.x * boom.x + boom.y * boom.y + boom.z * boom.z).sqrt();
            // LINT-OK: a boom of a few dozen meters, in quarter meters.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let steps = ((length / BOOM_STEP).ceil() as u32).clamp(1, 256);
            let along = |t: f32| {
                Vec3::new(
                    pivot.x + boom.x * t,
                    pivot.y + boom.y * t,
                    pivot.z + boom.z * t,
                )
            };
            for k in 1..=steps {
                // LINT-OK: a step count, at most 256.
                #[allow(clippy::cast_precision_loss)]
                let t = k as f32 / steps as f32;
                let q = along(t);
                if self
                    .ground_at(q.x, q.y)
                    .is_some_and(|g| q.z < g + clearance)
                {
                    // LINT-OK: as above.
                    #[allow(clippy::cast_precision_loss)]
                    let back = (k - 1) as f32 / steps as f32;
                    at = along(back);
                    break;
                }
            }
        }
        if let Some(g) = self.ground_at(at.x, at.y) {
            at.z = at.z.max(g + clearance);
        }
        Vec3::new(at.x - eye.x, at.y - eye.y, at.z - eye.z)
    }

    /// The drawn ground's height at render-space `(x, y)`: retail's, with the near offset (or
    /// the far one, by the distance from the eye), the water's bed and the floor of its banks.
    pub(crate) fn ground_at(&self, x: f32, y: f32) -> Option<f32> {
        let world = self.world.as_ref()?;
        let (gx, gy) = (x + self.global.0, y + self.global.1);
        let p = Point::at(world.as_ref(), gx, gy);
        let dist =
            ((x - self.eye.x).powi(2) + (y - self.eye.y).powi(2) + (p.retail - self.eye.z).powi(2))
                .sqrt();
        let far = drawn::morph(dist, &self.knobs);
        let near = self.near_at(gx, gy);
        Some(drawn::drawn_at(&p, gx, gy, near, far, self.water().as_ref(), &self.knobs).height)
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    if hi <= lo {
        return if x >= hi { 1.0 } else { 0.0 };
    }
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The near offset at global `(x, y)`, from the offset `tile` holds for the block there; nought
/// where it holds none.
fn near_at<'a>(tile: impl Fn((i32, i32)) -> Option<&'a [f32]>, x: f32, y: f32) -> f32 {
    let (bx, by) = (
        dereth_primitives::num::floor_to_i32(x / BLOCK_METERS),
        dereth_primitives::num::floor_to_i32(y / BLOCK_METERS),
    );
    // LINT-OK: block coordinates, far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    tile((bx, by)).map_or(0.0, |t| {
        drawn::sample(
            t,
            x - bx as f32 * BLOCK_METERS,
            y - by as f32 * BLOCK_METERS,
        )
    })
}

/// The water as the drawn ground lays it over `world` with `knobs`, while it is drawn as water.
fn water<'a>(world: &'a World, knobs: &Knobs) -> Option<drawn::Water<'a>> {
    knobs.on(Knob::Water).then(|| drawn::Water {
        shore: drawn::shore::Shore {
            side: WORLD_SIDE,
            heights: &world.heights,
            field: &world.water,
        },
        depth: knobs.get(Knob::WaterDepth),
        shelf: knobs.get(Knob::WaterShelf),
    })
}

/// How much of the shift a thing `above` meters over retail's ground keeps: all of it on the
/// ground, fading out from one height to the next.
fn keep(knobs: &Knobs, above: f32) -> f32 {
    1.0 - smoothstep(
        knobs.get(Knob::ShiftFade),
        knobs.get(Knob::ShiftGone).max(knobs.get(Knob::ShiftFade)),
        above,
    )
}

/// A block and its near offset.
type BlockOffset = ((i32, i32), Arc<Vec<f32>>);

/// What one block's scenery is baked moved by: the world's vertices, the knobs, and the near
/// offsets of the block and the blocks round it, as they stood when the block's bake was asked
/// for. Owned data, so a bake on another thread moves the scenery as one on the frame does.
#[derive(Debug, Clone)]
pub(crate) struct SceneryShift {
    block: (i32, i32),
    world: Arc<World>,
    knobs: Knobs,
    tiles: Vec<BlockOffset>,
}

impl SceneryShift {
    /// How far a piece of scenery baked at block-local `(x, y, z)` is drawn above where it
    /// stands: the near offset averaged over its stance, faded out as it stands higher over
    /// retail's ground, and never under the drawn ground there. The far ground is not baked into
    /// scenery: scenery stands near. Nought while bodies are not drawn on the drawn ground.
    pub(crate) fn shift(&self, x: f32, y: f32, z: f32) -> f32 {
        if !self.knobs.on(Knob::BodyShift) {
            return 0.0;
        }
        let world = self.world.as_ref();
        // LINT-OK: block coordinates, far inside f32's exact range.
        #[allow(clippy::cast_precision_loss)]
        let (gx, gy) = (
            self.block.0 as f32 * BLOCK_METERS + x,
            self.block.1 as f32 * BLOCK_METERS + y,
        );
        let p = Point::at(world, gx, gy);
        let keep = keep(&self.knobs, z - p.retail);
        if keep <= 0.0 {
            return 0.0;
        }
        let tile = |b: (i32, i32)| {
            self.tiles
                .iter()
                .find(|(k, _)| *k == b)
                .map(|(_, t)| t.as_slice())
        };
        let near = drawn::body_offset(|x, y| near_at(tile, x, y), gx, gy, &self.knobs);
        let water = water(world, &self.knobs);
        let ground = drawn::drawn_at(&p, gx, gy, near, 0.0, water.as_ref(), &self.knobs);
        drawn::shift(z, ground, keep)
    }
}

impl super::SceneDraw {
    /// The drawn ground's upkeep for this frame: its tuning, the world's vertices, whether it is
    /// drawn, the near blocks' offsets, and the blocks whose scenery was baked for another
    /// offset, which are baked again.
    pub(super) fn drawn_ground_step(
        &mut self,
        ws: &super::WorldState,
        store: &Arc<RetailDatStore>,
        gpu: &super::Gpu,
    ) {
        let settings = super::hifi_bridge::settings(&self.cfg.render.fidelity, false);
        let ground = self
            .land
            .art_pack
            .as_ref()
            .is_some_and(|p| !p.grounds.is_empty())
            && self.land.retail_ground.is_some();
        let presented = gpu.hifi_requested() && gpu.hifi_supported() && gpu.hifi_failed().is_none();
        self.drawn.step(
            settings.art_pack && ground && presented,
            store,
            &self.land.table,
        );
        if let Some((&(bx, by), b)) = self.blocks.iter().next() {
            // LINT-OK: block coordinates, far inside f32's exact range.
            #[allow(clippy::cast_precision_loss)]
            {
                self.drawn.global = (
                    bx as f32 * BLOCK_METERS - b.origin.0,
                    by as f32 * BLOCK_METERS - b.origin.1,
                );
            }
        }
        if let Some(v) = ws.streamer.window.viewer_block() {
            self.drawn.viewer = v;
        }
        self.drawn.eye = ws.camera.position;
        self.drawn.keep_fields(store);
        self.far_trees_step(store);
        let want = if self.drawn.active {
            self.drawn.bake_generation
        } else {
            0
        };
        let stale: Vec<(i32, i32)> = self
            .blocks
            .iter()
            .filter(|(_, b)| b.baked && b.drawn_baked != want)
            .map(|(k, _)| *k)
            .collect();
        for block in stale {
            if let Some((xi, yi)) = self.slot_of_block(ws, block) {
                self.want(ws, xi, yi, super::SlotWork::Full);
            }
        }
    }

    /// The far trees' upkeep for this frame: drawn while the drawn ground is, with an art pack
    /// that has far pictures.
    fn far_trees_step(&mut self, store: &Arc<RetailDatStore>) {
        let pack = self.land.art_pack.clone();
        let region = || Arc::new((*self.land.region).clone());
        let eye = (
            self.drawn.eye.x + self.drawn.global.0,
            self.drawn.eye.y + self.drawn.global.1,
        );
        self.far.step(
            self.drawn.active,
            pack.as_ref(),
            self.drawn.world(),
            &self.drawn.knobs,
            store,
            &region,
            &self.land.table,
            eye,
        );
    }
}

impl super::SceneDraw {
    /// Read the drawn ground's knobs from `path` from now on (see
    /// [`super::WorldScene::set_terrain_tuning_file`]).
    pub(crate) fn set_terrain_tuning_file(&mut self, path: Option<PathBuf>) {
        self.drawn.set_tuning_file(path);
    }

    /// For a report: the hash of the tuning file as it was last read, once it has been, and the
    /// knobs that differ from their defaults, each with its value as the file writes it.
    #[must_use]
    pub fn report_knobs(&self) -> (Option<String>, Vec<(String, String)>) {
        let hash = self
            .drawn
            .tuning
            .text
            .as_deref()
            .map(|t| dereth_client_runtime::report::ContentHash::of(t.as_bytes()));
        let changed = self
            .drawn
            .knobs
            .changed_from(&Knobs::default())
            .into_iter()
            .map(|k| {
                let value = self.drawn.knobs.get(k);
                let value = if k.spec().switch {
                    (value >= 0.5).to_string()
                } else {
                    value.to_string()
                };
                (k.spec().name.to_owned(), value)
            })
            .collect();
        (hash, changed)
    }
}

impl super::WorldScene {
    /// Read the drawn ground's knobs from `path` from now on, read again whenever it changes,
    /// and written with every knob at its default when there is no such file; `None` leaves the
    /// knobs as they are.
    pub fn set_terrain_tuning_file(&mut self, path: Option<PathBuf>) {
        self.draw.drawn.set_tuning_file(path);
    }

    /// The file the drawn ground's knobs are read from, where one is set.
    #[must_use]
    pub fn terrain_tuning_file(&self) -> Option<&std::path::Path> {
        self.draw.drawn.tuning_file()
    }

    /// Set the drawn ground's knobs, as an edit of the tuning file would.
    pub fn set_terrain_knobs(&mut self, knobs: Knobs) {
        self.draw.drawn.set_knobs(knobs);
    }

    /// The drawn ground's knobs.
    #[must_use]
    pub fn terrain_knobs(&self) -> &Knobs {
        &self.draw.drawn.knobs
    }

    /// Wait for the world's landscape vertices the drawn ground is drawn from, where they are
    /// being read: for a capture that has to draw it from its first frames.
    pub fn wait_for_drawn_ground(&mut self) {
        self.draw.drawn.wait_for_world();
    }

    /// Wait for the art pack's far trees within reach to be worked out, where they are being
    /// worked out: for a capture that has to draw them from its first frames. The drawn ground
    /// is waited for first; the frame after this draws them.
    pub fn wait_for_far_trees(&mut self) {
        self.draw.drawn.wait_for_world();
        self.draw.far.wait();
    }

    /// The far trees worked out so far: the blocks, the trees within reach, the milliseconds
    /// the thread spent working the blocks out and the milliseconds the frames spent keeping
    /// them.
    #[must_use]
    pub fn far_trees_counts(&self) -> (u64, usize, f64, f64) {
        let f = &self.draw.far;
        (
            f.taken.0,
            f.within().map(|(_, t)| t.len()).sum(),
            f.taken.1,
            f.taken.2,
        )
    }

    /// Whether the presentation draws the drawn ground, and the scene what stands on it moved
    /// with it.
    #[must_use]
    pub fn drawn_ground_active(&self) -> bool {
        self.draw.drawn.active
    }

    /// How far a thing at render-space `at` is drawn above where physics puts it.
    #[must_use]
    pub fn drawn_shift_at(&self, at: Vec3) -> f32 {
        self.draw.drawn.shift_at(at)
    }

    /// The drawn ground's height at render-space `(x, y)`, once the world's vertices are read.
    #[must_use]
    pub fn drawn_ground_at(&self, x: f32, y: f32) -> Option<f32> {
        self.draw.drawn.ground_at(x, y)
    }
}

impl super::WorldScene {
    /// What the drawn ground holds about the point under render-space `at`, as a line of text:
    /// for the captures that check it.
    #[must_use]
    pub fn drawn_ground_probe(&self, at: Vec3) -> String {
        let d = &self.draw.drawn;
        let Some(world) = d.world.as_ref() else {
            return "the world's vertices are not read".to_owned();
        };
        let (gx, gy) = (at.x + d.global.0, at.y + d.global.1);
        let p = Point::at(world.as_ref(), gx, gy);
        format!(
            "active {} global ({:.1}, {:.1}) world ({gx:.1}, {gy:.1}) at z {:.2} retail {:.2} smooth {:.2} \
             near {:.3} shift {:.3} ground {:?} tiles {} viewer {:?} offsets worked out {} in {:.0} ms, the world's vertices read in {:.0} ms",
            d.active,
            d.global.0,
            d.global.1,
            at.z,
            p.retail,
            p.smooth,
            d.near_at(gx, gy),
            d.shift_at(at),
            d.ground_at(at.x, at.y),
            d.fields.len(),
            d.viewer,
            d.worked.0,
            d.worked.1,
            world.read_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the drawn ground, an art-pack presentation's own)
    use super::*;

    /// A drawn ground over a world of rolling grass, drawn, with the near offset of block
    /// (100, 100) worked out and render space's origin at that block's south-west corner.
    fn rolling(knobs: Knobs) -> DrawnGround {
        let mut heights = vec![0.0f32; WORLD_SIDE * WORLD_SIDE];
        for gy in 780..840usize {
            for gx in 780..840usize {
                // LINT-OK: vertex coordinates, exact in f32.
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (gx as f32, gy as f32);
                let h = 100.0
                    + 14.0 * dereth_primitives::num::math::sinf(x * 0.37)
                    + 10.0 * dereth_primitives::num::math::cosf(y * 0.29);
                heights[gy * WORLD_SIDE + gx] = (h / 2.0).round() * 2.0;
            }
        }
        let words = vec![1 << 2; WORLD_SIDE * WORLD_SIDE];
        let water = drawn::shore::held(&drawn::shore::water_field(WORLD_SIDE, &heights, &words));
        let mut d = DrawnGround {
            knobs,
            world: Some(Arc::new(World {
                heights: Arc::new(heights),
                words: Arc::new(words),
                water: Arc::new(water),
                read_ms: 0.0,
            })),
            active: true,
            global: (100.0 * 192.0, 100.0 * 192.0),
            viewer: (100, 100),
            ..DrawnGround::default()
        };
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        let field = drawn::block_field(world.as_ref(), (100, 100), &[], &d.knobs);
        d.fields.insert(
            (100, 100),
            Tile {
                generation: 1,
                offset: Arc::new(field.offset),
                weight: Arc::new(field.weight),
            },
        );
        d
    }

    /// Scenery baked for the drawn ground, on the frame or off it, stands on the drawn ground as a
    /// body standing there is drawn, and a piece high over the ground is not moved.
    #[test]
    fn scenery_baked_with_the_drawn_grounds_shift_stands_on_the_drawn_ground() {
        let mut k = Knobs::default();
        k.set(Knob::WalkLowpass, 0.0);
        let d = rolling(k);
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        // Taken as a bake is asked for, then used where the scene is out of reach.
        let shift = std::thread::spawn({
            let shift = d
                .scenery_shift_held((100, 100))
                .expect("the block's offset");
            move || {
                [(10.0, 20.0), (57.5, 131.25), (96.0, 96.0), (180.0, 7.0)].map(|(x, y)| {
                    let retail =
                        Point::at(world.as_ref(), x + 100.0 * 192.0, y + 100.0 * 192.0).retail;
                    (
                        x,
                        y,
                        retail,
                        shift.shift(x, y, retail),
                        shift.shift(x, y, retail + 20.0),
                    )
                })
            }
        })
        .join()
        .expect("the thread");
        let mut largest = 0.0f32;
        for (x, y, retail, on, over) in shift {
            let ground = d.ground_at(x, y).expect("the drawn ground");
            assert!(
                (retail + on - ground).abs() < 1e-3,
                "({x}, {y}): {} on {ground}",
                retail + on
            );
            assert!(
                over.abs() < 1e-6,
                "({x}, {y}): moved {over} over the ground"
            );
            largest = largest.max(on.abs());
        }
        assert!(
            largest > 0.05,
            "the drawn ground leaves retail's here: {largest}"
        );
        // A block whose offset is not worked out has no shift to bake with.
        assert!(d.scenery_shift_held((101, 100)).is_none());
    }

    #[test]
    fn a_body_and_an_item_standing_on_retails_ground_are_drawn_on_the_drawn_ground() {
        let mut k = Knobs::default();
        k.set(Knob::WalkLowpass, 0.0);
        let d = rolling(k);
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        let mut largest = 0.0f32;
        for (x, y) in [(10.0, 20.0), (57.5, 131.25), (96.0, 96.0), (180.0, 7.0)] {
            // Where physics puts a thing standing there: on retail's triangle.
            let retail = Point::at(world.as_ref(), x + d.global.0, y + d.global.1).retail;
            let at = Vec3::new(x, y, retail);
            let drawn_at = retail + d.shift_at(at);
            let ground = d.ground_at(x, y).expect("the drawn ground");
            assert!(
                (drawn_at - ground).abs() < 1e-3,
                "({x}, {y}): {drawn_at} on {ground}"
            );
            largest = largest.max((ground - retail).abs());
        }
        assert!(
            largest > 0.05,
            "the drawn ground leaves retail's here: {largest}"
        );
        // A thing high over the ground (a bird, a missile's arc) is drawn where it is.
        let retail = Point::at(world.as_ref(), 57.5 + d.global.0, 131.25 + d.global.1).retail;
        assert!(d.shift_at(Vec3::new(57.5, 131.25, retail + 20.0)).abs() < 1e-6);
        // With the stance averaged, a body stands within a few centimeters of the ground.
        let soft = rolling(Knobs::default());
        let at = Vec3::new(57.5, 131.25, retail);
        let ground = soft.ground_at(57.5, 131.25).expect("the drawn ground");
        assert!((retail + soft.shift_at(at) - ground).abs() < 0.05);
    }

    /// A drawn ground over a river running north through block (100, 100): a hillside rising to
    /// the west whose foot meets a flat bank, where the rounded ground dips under retail's 36
    /// meters from the water; the river at 28 meters; a bank rising to the east. Drawn, with the
    /// block's near offset worked out.
    fn river(knobs: Knobs) -> DrawnGround {
        let mut heights = vec![0.0f32; WORLD_SIDE * WORLD_SIDE];
        let mut words = vec![1u16 << 2; WORLD_SIDE * WORLD_SIDE];
        for gy in 760..860usize {
            for gx in 760..860usize {
                // LINT-OK: vertex coordinates, exact in f32.
                #[allow(clippy::cast_precision_loss)]
                let x = gx as f32;
                let (h, water) = match gx {
                    0..=803 => (28.0 + 8.0 * (804.0 - x), false),
                    804..=805 => (28.0, false),
                    806..=813 => (28.0, true),
                    _ => (28.0 + 2.0 * (x - 813.0), false),
                };
                heights[gy * WORLD_SIDE + gx] = h;
                if water {
                    words[gy * WORLD_SIDE + gx] = 17 << 2;
                }
            }
        }
        let water = drawn::shore::held(&drawn::shore::water_field(WORLD_SIDE, &heights, &words));
        let mut d = DrawnGround {
            knobs,
            world: Some(Arc::new(World {
                heights: Arc::new(heights),
                words: Arc::new(words),
                water: Arc::new(water),
                read_ms: 0.0,
            })),
            active: true,
            global: (100.0 * 192.0, 100.0 * 192.0),
            viewer: (100, 100),
            ..DrawnGround::default()
        };
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        let field = drawn::block_field(world.as_ref(), (100, 100), &[], &d.knobs);
        d.fields.insert(
            (100, 100),
            Tile {
                generation: 1,
                offset: Arc::new(field.offset),
                weight: Arc::new(field.weight),
            },
        );
        d
    }

    #[test]
    fn a_body_beside_and_in_the_water_stands_on_the_ground_drawn_there() {
        let mut k = Knobs::default();
        k.set(Knob::WalkLowpass, 0.0);
        let d = river(k);
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        let water = drawn::Water {
            shore: drawn::shore::Shore {
                side: WORLD_SIDE,
                heights: &world.heights,
                field: &world.water,
            },
            depth: d.knobs.get(Knob::WaterDepth),
            shelf: d.knobs.get(Knob::WaterShelf),
        };
        let (mut dipped, mut bedded, mut sunk) = (0, 0, 0);
        for k in 0..200u8 {
            let x = 40.0 + f32::from(k) * 0.75;
            let y = 96.5;
            let (gx, gy) = (x + d.global.0, y + d.global.1);
            let p = Point::at(world.as_ref(), gx, gy);
            // The ground as the presentation draws it there, its own arithmetic.
            let near = d.near_at(gx, gy);
            let ground = drawn::drawn_at(&p, gx, gy, near, 0.0, Some(&water), &d.knobs).height;
            assert!(
                (d.ground_at(x, y).expect("the drawn ground") - ground).abs() < 1e-4,
                "at {x} the scene's ground is the presentation's"
            );
            // A body on retail's ground is drawn on the drawn ground, the floor beside the water
            // and the dip at the hill's foot alike.
            if water.shore.distance(gx, gy, 0.0) <= 0.0 {
                let feet = p.retail + d.shift_at(Vec3::new(x, y, p.retail));
                assert!(
                    (feet - ground).abs() < 1e-3,
                    "at {x} a body on land stands at {feet} on ground at {ground}"
                );
                if near < -0.2 {
                    dipped += 1;
                }
                continue;
            }
            // A body that retail sinks into its water stands no deeper than the bed under it,
            // and as deep as retail sinks it where the bed lies deeper.
            for sink in [0.1, 0.45, 0.9] {
                let z = p.retail - sink;
                let feet = z + d.shift_at(Vec3::new(x, y, z));
                assert!(
                    feet >= ground - 1e-3,
                    "at {x} a body sunk {sink} m stands at {feet} under the bed at {ground}"
                );
                if ground <= z + near {
                    assert!((feet - z - near).abs() < 1e-3, "at {x}: {feet} for {z}");
                    sunk += 1;
                } else {
                    assert!((feet - ground).abs() < 1e-3, "at {x}: {feet} on {ground}");
                    bedded += 1;
                }
            }
        }
        assert!(dipped > 5, "the rounding dips at the hill's foot: {dipped}");
        assert!(
            bedded > 3,
            "a body stands on a bed shallower than retail sinks it: {bedded}"
        );
        assert!(sunk > 20, "and sinks into deep water: {sunk}");
    }

    #[test]
    fn the_camera_keeps_its_clearance_over_the_drawn_ground() {
        let d = rolling(Knobs::default());
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        for (x, y) in [(10.0, 20.0), (57.5, 131.25), (96.0, 96.0)] {
            let ground = d.ground_at(x, y).expect("the drawn ground");
            let retail = Point::at(world.as_ref(), x + d.global.0, y + d.global.1).retail;
            // A camera kept just off retail's ground by physics, following no body and
            // following one beside it.
            let eye = Vec3::new(x, y, retail + 0.05);
            let clearance = d.knobs.get(Knob::CameraClearance);
            for body in [None, Some(Vec3::new(x + 3.0, y, retail))] {
                let drawn = eye.z + d.camera_offset(eye, body).z;
                assert!(
                    drawn >= ground + clearance - 1e-4,
                    "({x}, {y}), {body:?}: {drawn} over {ground}"
                );
            }
        }
        // Not drawn: the camera is drawn where it is.
        let mut off = rolling(Knobs::default());
        off.active = false;
        assert_eq!(
            off.camera_offset(Vec3::new(10.0, 20.0, 50.0), None),
            Vec3::ZERO
        );
    }

    #[test]
    fn a_camera_following_a_body_rises_with_the_body_so_it_glides_over_retails_facets() {
        let d = rolling(Knobs::default());
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        // A body walking east across the block, on retail's triangles, and a camera eight meters
        // behind it and three up, where physics puts them.
        let mut largest = 0.0f32;
        let mut riding = 0;
        for k in 0..60u8 {
            let x = 40.0 + f32::from(k) * 0.5;
            let feet = Vec3::new(
                x,
                100.0,
                Point::at(world.as_ref(), x + d.global.0, 100.0 + d.global.1).retail,
            );
            let eye = Vec3::new(x - 8.0, 100.0, feet.z + 3.0);
            let offset = d.camera_offset(eye, Some(feet));
            let body = d.shift_at(feet);
            // Where the drawn ground leaves the camera room, it rises exactly as the body does:
            // the camera keeps its place relative to the body as it is drawn.
            let ground = d.ground_at(eye.x, eye.y).expect("the drawn ground");
            let drawn_in = offset.x.abs() > 1e-6 || offset.y.abs() > 1e-6;
            if !drawn_in && eye.z + body > ground + 1.0 {
                assert!(
                    (offset.z - body).abs() < 1e-4,
                    "at {x}: the camera rises {} and the body {body}",
                    offset.z
                );
                riding += 1;
            }
            largest = largest.max(body.abs());
        }
        assert!(riding > 30, "{riding} of the steps checked");
        assert!(
            largest > 0.05,
            "the drawn ground leaves retail's: {largest}"
        );
    }

    #[test]
    fn the_drawn_ground_rising_between_a_body_and_its_camera_draws_the_camera_in() {
        let d = rolling(Knobs::default());
        let world = Arc::clone(d.world.as_ref().expect("a world"));
        let retail =
            |x: f32, y: f32| Point::at(world.as_ref(), x + d.global.0, y + d.global.1).retail;
        // A camera sunk under the ground twelve meters behind a body: the line from the body's
        // pivot to it runs into the drawn ground, and it is drawn in front of where it does,
        // nearer the body and over the ground.
        let feet = Vec3::new(96.0, 96.0, retail(96.0, 96.0));
        let eye = Vec3::new(84.0, 96.0, retail(84.0, 96.0) - 2.0);
        let o = d.camera_offset(eye, Some(feet));
        let at = Vec3::new(eye.x + o.x, eye.y + o.y, eye.z + o.z);
        assert!(at.x > eye.x + 0.2, "drawn in toward the body: {at:?}");
        let ground = d.ground_at(at.x, at.y).expect("the drawn ground");
        assert!(at.z >= ground + d.knobs.get(Knob::CameraClearance) - 1e-4);
        // A camera well over the ground is not drawn in.
        let high = Vec3::new(84.0, 96.0, feet.z + 30.0);
        let o = d.camera_offset(high, Some(feet));
        let at = Vec3::new(high.x + o.x, high.y + o.y, high.z + o.z);
        assert!((at.x - high.x).abs() < 1e-4 && (at.y - high.y).abs() < 1e-4);
    }

    #[test]
    fn the_tuning_file_is_read_again_whenever_what_it_holds_changes() {
        let dir =
            std::env::temp_dir().join(format!("dereth-terrain-tuning-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("terrain-tuning.toml");
        let mut d = DrawnGround::default();
        d.set_tuning_file(Some(file.clone()));
        let poll = |d: &mut DrawnGround| {
            for _ in 0..TUNING_EVERY {
                d.poll_tuning();
            }
        };
        // No file: one is written, with every knob at its default.
        poll(&mut d);
        let written = std::fs::read_to_string(&file).expect("the file is written");
        assert_eq!(Knobs::parse(&written), (Knobs::default(), Vec::new()));
        // An edit is taken, and the offsets it shapes are thrown away.
        let before = d.field_generation;
        std::fs::write(&file, "walk_cap = 0.5\ncliffs = false\n").expect("an edit");
        poll(&mut d);
        assert!((d.knobs.get(Knob::WalkCap) - 0.5).abs() < 1e-6);
        assert!(!d.knobs.on(Knob::Cliffs));
        assert_eq!(d.field_generation, before + 1);
        // And the next one.
        std::fs::write(&file, "walk_cap = 0.25\n").expect("an edit");
        poll(&mut d);
        assert!((d.knobs.get(Knob::WalkCap) - 0.25).abs() < 1e-6);
        assert!(
            d.knobs.on(Knob::Cliffs),
            "a knob the file no longer names is its default"
        );
        // A knob that shapes only what the presentation draws keeps the offsets.
        let before = d.field_generation;
        std::fs::write(&file, "walk_cap = 0.25\nmicro = 0.1\n").expect("an edit");
        poll(&mut d);
        assert_eq!(d.field_generation, before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
