//! The ground the remaster draws, the scene's half: the world's landscape vertices its grid of
//! patches is drawn from, and, while it draws the visual offset over retail's surface (see
//! [`dereth_terrain::land::drawn`]), each near block's offset and where what stands on the ground
//! is drawn on it: bodies, items, particles, the targeting ring, the scenery and the camera.
//!
//! Collision, physics, line of sight, picking and the server never see the offset. Buildings,
//! static objects, roads and water pin it to nought, so they and their rooms stand where they
//! always did.
//!
//! What stands on the ground is drawn standing on it at every distance: near the eye on the near
//! offset, far off on the far ground, and in between on the two mixed as the ground mixes them.
//! A piece of scenery carries how far it is drawn above where it stands on each, and the
//! remaster mixes the two by its distance from the eye frame by frame.

use std::collections::HashMap;
use std::sync::{mpsc, Arc};

use dereth_assets::Decode as _;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame, Vec3};
use dereth_remaster::scene as rm;
use dereth_terrain::land::drawn::{
    self, GroundSource, Knob, Knobs, Pin, Point, Vertex, BLOCK_METERS,
};

/// The world's landscape vertices a side.
pub(crate) const WORLD_SIDE: usize = 2041;
/// The height above a model's base its footprint is taken from: what meets the ground.
const FOOT: f32 = 2.5;
/// How far above a body's feet the camera that follows it pivots, meters: about its eyes.
const CAMERA_PIVOT: f32 = 1.6;
/// How far apart the camera's line from that pivot is tried against the drawn ground, meters.
const BOOM_STEP: f32 = 0.25;
/// The most blocks whose offset is worked out in one frame.
const FIELDS_A_FRAME: usize = 6;
/// The most bodies the fine relief is leveled under.
pub(crate) const LEVELED_BODIES: usize = 32;
/// How far from the eye a body's ground is leveled, meters: past it the fine relief is too small
/// to see under a body.
pub(crate) const LEVELED_WITHIN: f32 = 80.0;

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

/// The world's landscape vertices, `WORLD_SIDE` a side, row by row from the south.
#[derive(Debug)]
pub(crate) struct World {
    pub(crate) heights: Arc<Vec<f32>>,
    pub(crate) words: Arc<Vec<u16>>,
    /// Where the water lies about each vertex and its level (see [`drawn::shore`]).
    pub(crate) water: Arc<Vec<[f32; 2]>>,
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
fn read_world(store: &RetailDatStore, table: &[f32; 256]) -> World {
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

/// One block's near offset.
#[derive(Debug, Clone)]
pub(crate) struct Tile {
    /// A number of its own, so the remaster sees a tile worked out again.
    pub(crate) generation: u64,
    /// The offset bodies stand on, [`drawn::SIDE`] a side.
    pub(crate) offset: Arc<Vec<f32>>,
    /// How much of it the pins leave.
    pub(crate) weight: Arc<Vec<f32>>,
}

/// The remaster's ground in the scene.
#[derive(Debug, Default)]
pub(crate) struct RemasterGround {
    knobs: Knobs,
    world: Option<Arc<World>>,
    world_serial: u64,
    loading: Option<mpsc::Receiver<World>>,
    fields: HashMap<(i32, i32), Tile>,
    /// The number the next tile takes.
    next_tile: u64,
    pins: HashMap<(i32, i32), Arc<Vec<Pin>>>,
    footprints: HashMap<DataId, Option<[f32; 4]>>,
    /// Bumped whenever the offsets are thrown away for new knobs.
    field_generation: u64,
    /// Whether the offset is drawn this frame.
    pub(crate) active: bool,
    /// Where render space's origin lies in the world, meters east and north.
    pub(crate) global: (f32, f32),
    /// The viewer's block.
    pub(crate) viewer: (i32, i32),
    /// The eye, render space.
    pub(crate) eye: Vec3,
}

impl RemasterGround {
    /// The knobs the ground is drawn with.
    pub(crate) fn knobs(&self) -> &Knobs {
        &self.knobs
    }

    /// Draw the ground with `knobs` from now on.
    pub(crate) fn set_knobs(&mut self, knobs: Knobs) {
        let changed = knobs.changed_from(&self.knobs);
        self.knobs = knobs;
        if changed.iter().any(|k| FIELD_KNOBS.contains(k)) {
            self.fields.clear();
            self.pins.clear();
            self.field_generation += 1;
        }
    }

    /// One frame's upkeep: the world's vertices, asked for from `store` when the grid wants them
    /// and they have not been; whether the offset is drawn; and the far ground, drawn only out
    /// to the `horizon`.
    pub(crate) fn step(
        &mut self,
        (grid, offset, horizon): (bool, bool, bool),
        store: &Arc<RetailDatStore>,
        table: &[f32; 256],
    ) {
        if self.knobs.on(Knob::Far) != horizon {
            let mut k = self.knobs.clone();
            k.set(Knob::Far, if horizon { 1.0 } else { 0.0 });
            self.set_knobs(k);
        }
        if grid && self.world.is_none() && self.loading.is_none() {
            let (send, receive) = mpsc::channel();
            let store = Arc::clone(store);
            let table = *table;
            let spawned = std::thread::Builder::new()
                .name("landscape vertices".to_owned())
                .spawn(move || {
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
        self.active = grid && offset && self.knobs.on(Knob::Ground) && self.world.is_some();
    }

    /// Wait for the world's vertices, where they are being read, and take them.
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
    fn field(&mut self, store: &RetailDatStore, block: (i32, i32)) -> Option<Tile> {
        if let Some(t) = self.fields.get(&block) {
            return Some(t.clone());
        }
        let world = Arc::clone(self.world.as_ref()?);
        let pins = self.pins_for(store, block);
        let field = drawn::block_field(world.as_ref(), block, &pins, &self.knobs);
        self.next_tile += 1;
        let tile = Tile {
            generation: self.next_tile,
            offset: Arc::new(field.offset),
            weight: Arc::new(field.weight),
        };
        self.fields.insert(block, tile.clone());
        Some(tile)
    }

    /// How many blocks the near offset reaches from the viewer's, each way.
    pub(crate) fn reach(&self) -> u8 {
        // LINT-OK: a knob of 1 to 6.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let reach = self.knobs.get(Knob::NearBlocks).round() as u8;
        reach
    }

    /// Work out the offsets of the blocks round the viewer's that lack one, a few a frame, and
    /// forget those far from it.
    pub(crate) fn keep_fields(&mut self, store: &RetailDatStore) {
        if !self.active {
            return;
        }
        let reach = i32::from(self.reach());
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

    /// The offset of block `b`, when it is one the remaster draws: one within the near reach of
    /// the viewer's.
    fn near_tile(&self, b: (i32, i32)) -> Option<&Tile> {
        let reach = i32::from(self.reach());
        ((b.0 - self.viewer.0).abs() <= reach && (b.1 - self.viewer.1).abs() <= reach)
            .then(|| self.fields.get(&b))
            .flatten()
    }

    /// The near offsets the remaster draws, for the viewer's block and the blocks round it.
    pub(crate) fn near_tiles(&self) -> Vec<rm::DrawnTile> {
        self.fields
            .iter()
            .filter(|(b, _)| self.near_tile(**b).is_some())
            .filter_map(|(&(bx, by), t)| {
                Some(rm::DrawnTile {
                    block: (u16::try_from(bx).ok()? << 8) | u16::try_from(by).ok()?,
                    generation: t.generation,
                    offset: Arc::clone(&t.offset),
                    weight: Arc::clone(&t.weight),
                })
            })
            .collect()
    }

    /// The knobs as the remaster draws the ground with them.
    pub(crate) fn drawn_knobs(&self) -> rm::DrawnKnobs {
        let k = &self.knobs;
        rm::DrawnKnobs {
            detail: k.get(Knob::Detail),
            micro: (k.get(Knob::Micro), k.get(Knob::MicroScale)),
            level: k
                .on(Knob::Level)
                .then(|| (k.get(Knob::LevelRadius), k.get(Knob::LevelFade))),
            terraces: k.on(Knob::Terraces).then(|| {
                (
                    k.get(Knob::TerraceBand),
                    k.get(Knob::TerraceCap),
                    k.get(Knob::TerraceFrom),
                )
            }),
            morph: (k.get(Knob::MorphStart), k.get(Knob::MorphEnd)),
            far: k.on(Knob::Far).then(|| {
                (
                    k.get(Knob::FarRound),
                    k.get(Knob::FarDetail),
                    k.get(Knob::FarDetailScale),
                    k.get(Knob::Horizon),
                )
            }),
            haze: k.get(Knob::Haze),
            clouds: (k.on(Knob::Sky) && k.on(Knob::Clouds)).then(|| rm::DrawnClouds {
                cover: k.get(Knob::CloudCover),
                height: k.get(Knob::CloudHeight),
                depth: k.get(Knob::CloudDepth),
                scale: k.get(Knob::CloudScale),
                shadows: k.get(Knob::CloudShadows),
            }),
            water: k
                .on(Knob::Water)
                .then(|| (k.get(Knob::WaterDepth), k.get(Knob::WaterShelf))),
            slope: k.on(Knob::Slope).then(|| rm::DrawnSlope {
                scree: (k.get(Knob::ScreeFrom), k.get(Knob::ScreeBlend)),
                rock: (k.get(Knob::RockFrom), k.get(Knob::RockBlend)),
                strength: k.get(Knob::SlopeStrength),
                smooth: k.get(Knob::SlopeSmooth),
                on_snow: k.get(Knob::SlopeOnSnow),
            }),
            bank: (k.get(Knob::BankGreen), k.get(Knob::ShoreWet)),
        }
    }

    /// How far the ground is drawn, while it is drawn past the blocks held.
    pub(crate) fn horizon(&self) -> Option<f32> {
        (self.active && self.knobs.on(Knob::Far)).then(|| self.knobs.get(Knob::Horizon))
    }

    /// The near offset at global `(x, y)`, from the block's offset where the remaster draws one;
    /// nought elsewhere.
    fn near_at(&self, x: f32, y: f32) -> f32 {
        let (bx, by) = (
            dereth_primitives::num::floor_to_i32(x / BLOCK_METERS),
            dereth_primitives::num::floor_to_i32(y / BLOCK_METERS),
        );
        // LINT-OK: block coordinates, far inside f32's exact range.
        #[allow(clippy::cast_precision_loss)]
        self.near_tile((bx, by)).map_or(0.0, |t| {
            drawn::sample(
                &t.offset,
                x - bx as f32 * BLOCK_METERS,
                y - by as f32 * BLOCK_METERS,
            )
        })
    }

    /// The water as the ground lays it, while it is drawn as water.
    fn water(&self) -> Option<drawn::Water<'_>> {
        let world = self.world.as_deref()?;
        self.knobs.on(Knob::Water).then(|| drawn::Water {
            shore: drawn::shore::Shore {
                side: WORLD_SIDE,
                heights: &world.heights,
                field: &world.water,
            },
            depth: self.knobs.get(Knob::WaterDepth),
            shelf: self.knobs.get(Knob::WaterShelf),
        })
    }

    /// How much of the shift a thing `above` meters over retail's ground keeps: all of it on the
    /// ground, fading out from one height to the next.
    fn keep(&self, above: f32) -> f32 {
        1.0 - smoothstep(
            self.knobs.get(Knob::ShiftFade),
            self.knobs
                .get(Knob::ShiftGone)
                .max(self.knobs.get(Knob::ShiftFade)),
            above,
        )
    }

    /// How far a thing standing at global `(x, y)` and height `z` is drawn above where physics
    /// puts it, `far` of the way given way to the far ground: the offset averaged over its
    /// stance, faded out as it stands higher over retail's ground; and never under the drawn
    /// ground there, the water's bed and the floor of its banks included.
    fn shift(&self, world: &World, (x, y, z): (f32, f32, f32), far: f32) -> f32 {
        let p = Point::at(world, x, y);
        let keep = self.keep(z - p.retail);
        if keep <= 0.0 {
            return 0.0;
        }
        let near = if far < 1.0 {
            drawn::body_offset(|x, y| self.near_at(x, y), x, y, &self.knobs)
        } else {
            0.0
        };
        let ground = drawn::drawn_at(&p, x, y, near, far, self.water().as_ref(), &self.knobs);
        drawn::shift(z, ground, keep)
    }

    /// How far a thing standing at render-space `at` is drawn above where physics puts it, given
    /// way to the far ground with its distance from the eye. Nought while the offset is not drawn.
    pub(crate) fn shift_at(&self, at: Vec3) -> f32 {
        if !self.active || !self.knobs.on(Knob::BodyShift) {
            return 0.0;
        }
        let Some(world) = self.world.as_ref() else {
            return 0.0;
        };
        let (x, y) = (at.x + self.global.0, at.y + self.global.1);
        let retail = Point::at(world.as_ref(), x, y).retail;
        let eye = self.eye;
        let dist =
            ((at.x - eye.x).powi(2) + (at.y - eye.y).powi(2) + (retail - eye.z).powi(2)).sqrt();
        let far = drawn::morph(dist, &self.knobs);
        self.shift(world, (x, y, at.z), far)
    }

    /// How far a piece of scenery standing at global `(x, y)` and height `z` is drawn above where
    /// it stands: on the near ground, and on the far ground. The remaster mixes the two by the
    /// piece's distance from the eye as the ground mixes them. Nought while the offset is not
    /// drawn.
    pub(crate) fn lifts(&self, x: f32, y: f32, z: f32) -> [f32; 2] {
        if !self.active || !self.knobs.on(Knob::BodyShift) {
            return [0.0; 2];
        }
        let Some(world) = self.world.as_ref() else {
            return [0.0; 2];
        };
        let near = self.shift(world, (x, y, z), 0.0);
        let far = if self.knobs.on(Knob::Far) {
            self.shift(world, (x, y, z), 1.0)
        } else {
            near
        };
        [near, far]
    }

    /// A number for what block `block`'s scenery is lifted by: it changes whenever the offsets
    /// its lifts are worked out from do. Nought while the offset is not drawn.
    pub(crate) fn lifts_key(&self, block: (i32, i32)) -> u64 {
        use std::hash::{Hash, Hasher};
        if !self.active {
            return 0;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.field_generation.hash(&mut h);
        self.knobs.on(Knob::Far).hash(&mut h);
        for x in block.0 - 1..=block.0 + 1 {
            for y in block.1 - 1..=block.1 + 1 {
                self.near_tile((x, y)).map(|t| t.generation).hash(&mut h);
            }
        }
        h.finish() | 1
    }

    /// How far the camera at render-space `eye` is drawn from where it stands, `body` the feet
    /// (render space, where physics puts them) of the body it follows, if any: it rises as the
    /// body does, is drawn in front of the drawn ground where the ground rises between it and
    /// the body, and never closer to the ground than the camera's clearance.
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

    /// The drawn ground's height at render-space `(x, y)`, without its fine relief: retail's,
    /// with the near offset (or the far one, by the distance from the eye), the water's bed and
    /// the floor of its banks. `None` before the world's vertices are read.
    pub(crate) fn ground_at(&self, x: f32, y: f32) -> Option<f32> {
        let world = self.world.as_ref()?;
        let (gx, gy) = (x + self.global.0, y + self.global.1);
        let p = Point::at(world.as_ref(), gx, gy);
        if !self.active {
            return Some(p.retail);
        }
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

impl super::SceneDraw {
    /// The remaster's ground's upkeep for this frame: the world's vertices, whether the offset
    /// is drawn, and the near blocks' offsets.
    ///
    /// It runs before the frame's re-centre, so [`Self::place_remaster_ground`] runs again after
    /// it.
    pub(super) fn remaster_ground_step(
        &mut self,
        ws: &super::WorldState,
        store: &Arc<RetailDatStore>,
    ) {
        let wanted = self.remaster_ground_wanted();
        self.remaster_ground.step(wanted, store, &self.land.table);
        self.place_remaster_ground(ws);
        self.remaster_ground.keep_fields(store);
        self.remaster_far_step(wanted.0, store);
    }

    /// Where the remaster's ground stands this frame: the world's origin in render space, the
    /// viewer's block and the eye, as the window holds the blocks now. A re-centre moves the
    /// blocks' origins, so this runs again after one; otherwise the frame that crosses into the
    /// next block reads the ground a block away.
    pub(super) fn place_remaster_ground(&mut self, ws: &super::WorldState) {
        if let Some((&(bx, by), b)) = self.blocks.iter().next() {
            // LINT-OK: block coordinates, far inside f32's exact range.
            #[allow(clippy::cast_precision_loss)]
            {
                self.remaster_ground.global = (
                    bx as f32 * BLOCK_METERS - b.origin.0,
                    by as f32 * BLOCK_METERS - b.origin.1,
                );
            }
        }
        if let Some(v) = ws.streamer.window.viewer_block() {
            self.remaster_ground.viewer = v;
        }
        self.remaster_ground.eye = ws.camera.position;
    }

    /// The far trees' and the forest cover's upkeep for this frame: while the ground is drawn as
    /// a grid, with an art pack whose trees are drawn.
    fn remaster_far_step(&mut self, grid: bool, store: &Arc<RetailDatStore>) {
        let view = self.remaster_view.get_mut();
        let pack = view
            .pack
            .as_ref()
            .filter(|_| {
                view.packs
                    .models(dereth_remaster::assets::pack::Category::Tree)
            })
            .map(|p| Arc::clone(p.pack()));
        let ground = &self.remaster_ground;
        let land = ground.world().map(|(w, serial)| {
            (
                super::far_trees::Land {
                    heights: Arc::clone(&w.heights),
                    words: Arc::clone(&w.words),
                },
                serial,
            )
        });
        let eye = (
            ground.eye.x + ground.global.0,
            ground.eye.y + ground.global.1,
        );
        let region = || Arc::new((*self.land.region).clone());
        self.remaster_far.step(
            grid,
            pack.as_ref(),
            land,
            ground.knobs(),
            store,
            &region,
            &self.land.table,
            eye,
        );
    }
}

impl super::WorldScene {
    /// Wait for the art pack's far trees and forest cover the remaster draws to be worked out,
    /// where they are being worked out: for a capture that has to draw them from its first
    /// frames. The frame after this draws them.
    pub fn wait_for_remaster_far_trees(&mut self) {
        self.draw.remaster_ground.wait_for_world();
        self.draw.remaster_far.wait();
    }

    /// Wait for the world's landscape vertices the remaster's ground is drawn from, where they
    /// are being read: for a capture that has to draw it from its first frames.
    pub fn wait_for_remaster_ground(&mut self) {
        self.draw.remaster_ground.wait_for_world();
    }

    /// Whether the remaster draws its ground's offset, and what stands on the ground on it.
    #[must_use]
    pub fn remaster_ground_active(&self) -> bool {
        self.draw.remaster_ground.active
    }

    /// The remaster's ground's height at render-space `(x, y)`, without its fine relief, once
    /// the world's vertices are read.
    #[must_use]
    pub fn remaster_ground_at(&self, x: f32, y: f32) -> Option<f32> {
        self.draw.remaster_ground.ground_at(x, y)
    }

    /// How far a thing at render-space `at` is drawn above where physics puts it, on the
    /// remaster's ground.
    #[must_use]
    pub fn remaster_shift_at(&self, at: Vec3) -> f32 {
        self.draw.remaster_ground.shift_at(at)
    }

    /// How far a piece of scenery standing at render-space `at` is drawn above it on the
    /// remaster's ground, near and far (see [`RemasterGround::lifts`]).
    #[must_use]
    pub fn remaster_lifts_at(&self, at: Vec3) -> [f32; 2] {
        let g = &self.draw.remaster_ground;
        g.lifts(at.x + g.global.0, at.y + g.global.1, at.z)
    }

    /// The remaster's ground's knobs.
    #[must_use]
    pub fn remaster_ground_knobs(&self) -> &Knobs {
        self.draw.remaster_ground.knobs()
    }

    /// Draw the remaster's ground with `knobs` from the next frame.
    pub fn set_remaster_ground_knobs(&mut self, knobs: Knobs) {
        self.draw.remaster_ground.set_knobs(knobs);
    }
}
