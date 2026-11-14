//! The bridge from the scene to the remaster.
//!
//! Three jobs, all read-only towards the game:
//! - **The bake's records.** A block's bake merges its objects' triangles by surface, so it keeps,
//!   beside them, where each placement and each of its parts stands ([`BakedPlacement`]), and
//!   what a room's draw does not keep of the room's record ([`RoomRecord`]).
//! - **The walk's record.** While a receiver is installed, the walk notes what it drew of the
//!   moving objects and of the landscape ([`WalkLog`]), at the points it decides to draw them.
//! - **The publisher.** After the frame's walk, [`publish`] compares the scene with what it last
//!   sent and hands the receiver the changes ([`remaster::SceneDelta`]) and the frame's own state
//!   ([`remaster::FrameDelta`]). It is built from shared reads after the walk has finished, so it
//!   cannot change what the frame drew or what can be picked in it. With no receiver installed
//!   nothing is built.
//!
//! The bridge's own memory between frames ([`Bridge`]) is handed to it by its caller, so nothing
//! here writes through a shared reference: the scene and the world are only read.

use std::collections::HashMap;

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, Frame, Vec3};

/// The remaster's plain-data scene types, for the scene's callers.
pub use dereth_remaster::scene as remaster;
use remaster as rm;

use super::{
    frame_matrix, swap_zup_to_d3d, AppearanceKey, BlockDraw, PartLevels, PartSubmission, SceneDraw,
    WorldState,
};

/// One placement as the bake placed it: the object, where it stands and each part it drew.
#[derive(Debug, Clone)]
pub(super) struct BakedPlacement {
    id: DataId,
    frame: Frame,
    scale: f32,
    building: bool,
    pub(super) parts: Vec<BakedPart>,
}

impl BakedPlacement {
    /// The placement of `id` at `frame`, scaled `scale`, a building's shell when `building`, with
    /// no part yet.
    pub(super) fn new(id: DataId, frame: Frame, scale: f32, building: bool) -> Self {
        Self {
            id,
            frame,
            scale,
            building,
            parts: Vec::new(),
        }
    }
}

/// One part a placement drew: its GfxObj, its frame and mesh scale, and the degrading placement
/// whose level it draws at, numbered as the bake that made it numbers them.
#[derive(Debug, Clone)]
pub(super) struct BakedPart {
    gfxobj: DataId,
    frame: Frame,
    scale: Vec3,
    pub(super) placement: Option<u32>,
}

impl BakedPart {
    pub(super) fn new(gfxobj: DataId, frame: Frame, scale: Vec3, placement: Option<u32>) -> Self {
        Self {
            gfxobj,
            frame,
            scale,
            placement,
        }
    }
}

/// What a room's draw does not keep of the room's record: the Environment piece its geometry
/// comes from, its surfaces, whether it sees the outdoors, and its statics as they were placed.
#[derive(Debug, Clone, Default)]
pub(super) struct RoomRecord {
    environment: u32,
    structure: u16,
    surfaces: Vec<u32>,
    seen_outside: bool,
    pub(super) placements: Vec<BakedPlacement>,
}

impl RoomRecord {
    pub(super) fn new(
        cell: &dereth_assets::world::EnvCell,
        placements: Vec<BakedPlacement>,
    ) -> Self {
        Self {
            environment: cell.environment.0,
            structure: cell.cell_struct,
            surfaces: cell.surfaces.iter().map(|s| s.0).collect(),
            seen_outside: cell.flags & 1 != 0,
            placements,
        }
    }
}

/// One object part the walk drew: whose, which, whether with the outdoor light, and the sphere
/// its lights were chosen by.
#[derive(Debug, Clone, Copy)]
pub(super) struct WalkPart {
    key: rm::ObjectKey,
    part: u32,
    outdoors: bool,
    centre: Vec3,
    radius: f32,
}

impl WalkPart {
    /// The part `s` the walk is drawing, the sphere its lights are chosen by worked out as the
    /// walk works it out.
    pub(super) fn of(s: &PartSubmission<'_>) -> Self {
        use dereth_world_render::cells::cull::object_scale;
        let (centre, radius) = match s.drawing_sphere {
            Some((c, r)) => {
                let scale = object_scale(s.part.gfxobj_scale);
                let scaled = Vec3::new(c.x * scale, c.y * scale, c.z * scale);
                (
                    dereth_terrain::math::localtoglobal(&s.draw_pos, scaled),
                    scale * r,
                )
            }
            None => (s.draw_pos.origin, 0.0),
        };
        let key = match (s.object, s.placed) {
            (Some(id), _) => rm::ObjectKey::Server(id.0),
            (None, Some((cell, placement))) => animated(cell.0, placement),
            (None, None) => rm::ObjectKey::Body,
        };
        Self {
            key,
            part: u32::try_from(s.index).unwrap_or(u32::MAX),
            outdoors: s.outdoors,
            centre,
            radius,
        }
    }
}

/// One particle the walk drew: its card's graphics object, where it stands as drawn, its look and
/// the sphere its lights are chosen by.
#[derive(Debug, Clone, Copy)]
pub(super) struct WalkParticle {
    gfx: u32,
    pub(super) drawn: crate::particles::ParticleDrawn,
}

impl WalkParticle {
    /// The prepared particle of graphics object `gfx`, as it is drawn.
    pub(super) fn of(gfx: DataId, drawn: &crate::particles::ParticleDrawn) -> Self {
        Self {
            gfx: gfx.0,
            drawn: *drawn,
        }
    }
}

/// What the walk drew, for the remaster: the moving objects' parts, the particles, the blocks of
/// the landscape, and whether it drew the outdoors at all.
#[derive(Debug, Default)]
pub(super) struct WalkLog {
    pub(super) parts: Vec<WalkPart>,
    pub(super) particles: Vec<WalkParticle>,
    pub(super) blocks: Vec<u16>,
    pub(super) landscape: bool,
}

/// The key of the animated static that is the `placement`th of its block's, in cell `cell`.
fn animated(cell: u32, placement: usize) -> rm::ObjectKey {
    rm::ObjectKey::Animated {
        // LINT-OK: the high half of a cell id is its block. Not a float conversion.
        #[allow(clippy::cast_possible_truncation)]
        block: (cell >> 16) as u16,
        index: u32::try_from(placement).unwrap_or(u32::MAX),
    }
}

/// The bridge's memory between frames: the receiver, and what it was last sent.
#[derive(Debug, Default)]
pub(crate) struct Bridge {
    /// Where the scene is published; nothing is built without one.
    pub(super) receiver: Option<Box<dyn rm::Receiver>>,
    /// What the receiver holds, as the bridge sent it.
    pub(super) shadow: Shadow,
}

impl Bridge {
    /// Whether a receiver is installed.
    pub(super) fn receiving(&self) -> bool {
        self.receiver.is_some()
    }
}

/// What the receiver holds: each block's generation and its levels' key, each object's build,
/// each object part's transform last frame, the last view and projection, and the frame's origin.
#[derive(Debug, Default)]
pub(crate) struct Shadow {
    // ORDER-OK: keyed lookups; the deltas are made in the window's order.
    blocks: HashMap<u16, u32>,
    // ORDER-OK: as above.
    levels: HashMap<u16, u64>,
    // ORDER-OK: as above. The choice of levels each block held when its levels were last read
    // ([`super::BlockDraw::levels_serial`]): they are read again only once it holds another.
    level_serials: HashMap<u16, u64>,
    // ORDER-OK: as above. Each block as it was last sent, its build's number aside.
    contents: HashMap<u16, rm::Block>,
    // ORDER-OK: as above. What each block's scenery was lifted by, as one number (0 for none).
    shifts: HashMap<u16, u64>,
    // ORDER-OK: as above. The ground cover each block last grew, and what it grew from.
    covers: HashMap<u16, Cover>,
    /// A block's ground cover grew whole on the last frame, and is sent on this one.
    cover_whole: bool,
    /// The world's vertices last sent, and their serial.
    drawn_world: Option<(u64, std::sync::Arc<rm::DrawnWorld>)>,
    /// The far forest last sent: the pack's far pictures, the trees within reach, and the cover.
    far_pack: Option<std::sync::Arc<rm::FarPack>>,
    far_trees: Option<std::sync::Arc<rm::FarTreeSet>>,
    far_cover: Option<std::sync::Arc<rm::ForestCover>>,
    // ORDER-OK: as above; the objects are published in the scene's own order.
    objects: HashMap<rm::ObjectKey, u64>,
    // ORDER-OK: as above.
    previous: HashMap<rm::ObjectKey, Vec<glam::Mat4>>,
    // ORDER-OK: as above. The near ground's offset under each animated static, kept with what it
    // was worked out from ([`super::remaster_ground::RemasterGround::still_shift_at`]).
    host_nears: HashMap<rm::ObjectKey, (u64, u32, f32)>,
    previous_view_projection: Option<glam::Mat4>,
    origin: Option<[i32; 2]>,
    serial: u64,
    /// The scene's clock at its last step, seconds.
    pub(super) now: f64,
}

/// About the most ground cover grown in one frame, placements, while the landscape streams within
/// a budget: a block's cover is thousands, each lifted onto the drawn ground, so a block's is grown
/// a row of cells at a time over a few frames, and the block is sent once it is whole.
const COVER_A_FRAME: usize = 2500;

/// What ground cover a frame may still grow, while the landscape streams within a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoverBudget {
    /// Placements still to grow this frame.
    left: usize,
    /// A block's cover grew whole this frame: it is sent on the next, which grows none, so the
    /// frame that sends a block whose cover grew over frames does not grow cover as well.
    whole: bool,
}

impl CoverBudget {
    /// A frame's budget: none the frame after a cover grew whole, when that block is sent.
    fn after(whole: bool) -> Self {
        Self {
            left: if whole { 0 } else { COVER_A_FRAME },
            whole: false,
        }
    }
}

/// What a block's ground cover grows from: its full-detail ground, its terrain, its buildings,
/// the covers and what they are lifted by. A block built again from the same grows the same.
#[derive(Debug, Clone, PartialEq)]
struct CoverInputs {
    vertices: Vec<Vec3>,
    cuts: Vec<bool>,
    water: Vec<dereth_terrain::land::water::WaterType>,
    words: [u16; 81],
    buildings: Vec<glam::Vec3>,
    covers: Vec<dereth_assets::art_pack::ManifestCover>,
    lifts: u64,
    knobs: dereth_terrain::land::drawn::Knobs,
}

/// The ground cover a block grew, and what it grew from: whole once every row is grown.
#[derive(Debug, Clone)]
pub(crate) struct Cover {
    inputs: CoverInputs,
    placements: Vec<rm::Placement>,
    /// The rows grown so far (see [`super::remaster_cover::cover_rows`]).
    rows: usize,
}

impl Cover {
    /// The cover `inputs` grow in `rows` rows, `grow` growing one onto what is grown: what
    /// `entry` holds when it grew from the same, or grown a row at a time while `allowance`
    /// lasts, at least a row (or all of it at once with none). `None` until it is whole, and
    /// within an allowance on the frame it grows whole too, which the allowance then counts
    /// ([`CoverBudget::whole`]); what is grown is kept for the next frame.
    fn grow<'a>(
        entry: std::collections::hash_map::Entry<'a, u16, Cover>,
        inputs: CoverInputs,
        rows: usize,
        mut allowance: Option<&mut CoverBudget>,
        mut grow: impl FnMut(&CoverInputs, usize, &mut Vec<rm::Placement>),
    ) -> Option<&'a [rm::Placement]> {
        // Grown again only from something new: a block is built again far more often than what
        // its cover grows from changes (its light, its neighbors' detail).
        let c = entry.or_insert_with(|| Cover {
            inputs: inputs.clone(),
            placements: Vec::new(),
            rows: 0,
        });
        if c.inputs != inputs {
            *c = Cover {
                inputs,
                placements: Vec::new(),
                rows: 0,
            };
        }
        let grown = c.rows < rows;
        while c.rows < rows {
            if allowance.as_deref().is_some_and(|b| b.left == 0) {
                return None;
            }
            let before = c.placements.len();
            grow(&c.inputs, c.rows, &mut c.placements);
            c.rows += 1;
            if let Some(b) = allowance.as_deref_mut() {
                b.left = b.left.saturating_sub((c.placements.len() - before).max(1));
            }
        }
        if let Some(b) = allowance.filter(|_| grown) {
            b.whole = true;
            return None;
        }
        Some(&c.placements)
    }
}

/// Whether `b` holds what `last` did, its ground and its build's number aside: a block built
/// again that does changed only its ground, if that.
fn same_contents(last: &rm::Block, b: &rm::Block) -> bool {
    last.placements == b.placements
        && last.degrades == b.degrades
        && last.rooms == b.rooms
        && last.lights == b.lights
        && last.lamps == b.lamps
}

/// What an object is built from, as one number: the setup, the appearance its meshes are shared
/// under, the GfxObj each part was built from, and the build of the block or body it belongs to.
/// A build from anything else has another number, and the object is sent again.
fn build_of(
    setup: DataId,
    appearance: Option<&AppearanceKey>,
    meshes: &[PartLevels],
    generation: u32,
) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    let mut mix = |v: u32| h = (h ^ u64::from(v)).wrapping_mul(0x0100_0000_01B3);
    mix(setup.0);
    mix(generation);
    mix(u32::try_from(meshes.len()).unwrap_or(u32::MAX));
    for p in meshes {
        mix(p.gfxobj.0);
    }
    if let Some(a) = appearance {
        mix(1);
        mix(a.setup.0);
        mix(a.palette);
        for &(id, offset, count) in &a.subpalettes {
            mix(id);
            mix(u32::from(offset) << 8 | u32::from(count));
        }
        mix(0xFFFF_FFFF);
        for &(part, old, new) in &a.textures {
            mix(u32::from(part));
            mix(old);
            mix(new);
        }
        mix(0xFFFF_FFFF);
        for &(part, id) in &a.parts {
            mix(u32::from(part));
            mix(id);
        }
    }
    h
}

pub(super) fn mat(f: &Frame) -> glam::Mat4 {
    frame_matrix(f)
}

/// The lights the walk's object lighting picks for a sphere, as indices into the frame's pools
/// (the static lights first), and how many.
fn light_indices(
    pools: &dereth_world_render::lighting::LightPools,
    centre: Vec3,
    radius: f32,
) -> ([u16; 8], u8) {
    let statics = pools.statics.len();
    let mut lights = [0u16; 8];
    let mut count = 0u8;
    let active = dereth_world_render::lighting::minimize_object_lighting(pools, centre, radius);
    for slot in &active.cur {
        let Ok(i) = usize::try_from(slot.index) else {
            continue;
        };
        let index = match slot.light_class {
            1 => i,
            2 => statics + i,
            _ => continue,
        };
        let at = usize::from(count);
        if let (true, Ok(index)) = (at < lights.len(), u16::try_from(index)) {
            lights[at] = index;
            count += 1;
        }
    }
    (lights, count)
}

pub(super) fn v3(v: Vec3) -> glam::Vec3 {
    glam::Vec3::new(v.x, v.y, v.z)
}

fn level_byte(level: u32) -> u8 {
    u8::try_from(level).unwrap_or(u8::MAX)
}

fn block_of((x, y): (i32, i32)) -> u16 {
    // LINT-OK: block indices are 0..=0xFE. Not a float conversion.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let id = ((x as u16) << 8) | (y as u16);
    id
}

/// The near and far planes a left-handed perspective transform was made with.
fn planes(projection: glam::Mat4) -> (f32, f32) {
    let a = projection.z_axis.z;
    let b = projection.w_axis.z;
    if a.abs() < f32::EPSILON || (a - 1.0).abs() < f32::EPSILON {
        return (0.1, 1000.0);
    }
    let near = -b / a;
    (near, a * near / (a - 1.0))
}

fn light_kind(t: dereth_world_render::lighting::LightType) -> rm::LightKind {
    use dereth_world_render::lighting::LightType;
    match t {
        LightType::Point => rm::LightKind::Point,
        LightType::Directional => rm::LightKind::Directional,
        LightType::Spot => rm::LightKind::Spot,
    }
}

fn material(m: Option<dereth_animation::parts::MaterialOverride>) -> Option<rm::Material> {
    m.map(|m| rm::Material {
        translucency: m.translucency,
        diffusion: m.diffuse,
        luminosity: m.luminosity,
    })
}

/// A placement as the remaster is sent it, its parts' degrade records moved up by `first`.
fn placement(p: &BakedPlacement, first: u32) -> rm::Placement {
    rm::Placement {
        id: p.id.0,
        lifts: [0.0; 2],
        kind: if p.building {
            rm::PlacementKind::Building
        } else {
            rm::PlacementKind::Object
        },
        frame: mat(&p.frame),
        scale: p.scale,
        parts: p
            .parts
            .iter()
            .map(|q| rm::Part {
                gfxobj: q.gfxobj.0,
                frame: mat(&q.frame),
                scale: v3(q.scale),
                degrade: q.placement.map(|i| first + i),
            })
            .collect(),
    }
}

fn degrade(p: &super::DegradePlacement) -> rm::Degrade {
    rm::Degrade {
        levels: p.info.degrades.iter().map(|e| e.gfxobj_id.0).collect(),
        billboards: p.billboards,
        bands: p
            .info
            .degrades
            .iter()
            .map(|e| [e.min_dist, e.ideal_dist, e.max_dist])
            .collect(),
        modes: p
            .info
            .degrades
            .iter()
            .map(|e| u8::try_from(e.degrade_mode).unwrap_or(1))
            .collect(),
        scale_z: p.scale_z,
        centre: v3(p.centre),
    }
}

/// Every degrading placement of `block`, its own first and then each room's in turn.
fn block_degrades(block: &BlockDraw) -> impl Iterator<Item = &super::DegradePlacement> {
    block
        .degrade
        .iter()
        .chain(block.env_cells.iter().flat_map(|c| &c.degrade))
}

/// `block` as the remaster is sent it, its ground as `mesh` draws it.
/// `block` (at `at`, the block's coordinates) as the remaster is sent it, its ground as `mesh`
/// draws it, each of its placements lifted onto the remaster's ground as `ground` draws it.
fn block_delta(
    (id, at): (u16, (i32, i32)),
    block: &BlockDraw,
    mesh: &dereth_terrain::land::mesh::LandblockMesh,
    ground: &super::remaster_ground::RemasterGround,
    store: Option<&RetailDatStore>,
    (covers, allowance): (
        &[dereth_assets::art_pack::ManifestCover],
        Option<&mut CoverBudget>,
    ),
    cache: &mut HashMap<u16, Cover>,
) -> Option<rm::Block> {
    let own = u32::try_from(block.degrade.len()).unwrap_or(u32::MAX);
    let mut first = own;
    let rooms = block
        .env_cells
        .iter()
        .map(|c| {
            let at = first;
            first = first.saturating_add(u32::try_from(c.degrade.len()).unwrap_or(u32::MAX));
            rm::Room {
                id: c.id.0,
                environment: c.remaster.environment,
                structure: c.remaster.structure,
                surfaces: c.remaster.surfaces.clone(),
                frame: mat(&c.frame),
                seen_outside: c.remaster.seen_outside,
                portals: c
                    .portals
                    .iter()
                    .map(|p| rm::Portal {
                        other: (p.other_cell_id
                            != dereth_world_render::cells::portal_view::OUTDOORS)
                            .then_some(p.other_cell_id),
                        polygon: p.vertices.iter().copied().map(v3).collect(),
                    })
                    .collect(),
                placements: c
                    .remaster
                    .placements
                    .iter()
                    .map(|p| placement(p, at))
                    .collect(),
            }
        })
        .collect();
    let lights = block
        .cell_lights
        .iter()
        .map(|l| {
            let at = dereth_terrain::math::combine(&l.frame, &l.info.offset);
            let placed = dereth_world_render::lighting::LightInfo {
                offset: at,
                ..l.info
            };
            rm::StaticLight {
                cell: l.cell.0,
                kind: light_kind(l.info.light_type),
                position: v3(at.origin),
                direction: v3(dereth_world_render::lighting::get_direction(&placed)),
                color: l.info.color,
                intensity: l.info.intensity,
                falloff: l.info.falloff,
                cone_angle: l.info.cone_angle,
                burned: l.is_static,
            }
        })
        .collect();
    let lamps = store.map_or_else(Vec::new, |store| block_lamps(store, at, block, ground));
    let mut placements: Vec<rm::Placement> = block
        .remaster_placements
        .iter()
        .map(|p| {
            let mut placed = placement(p, 0);
            // LINT-OK: block coordinates, far inside f32's exact range.
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                at.0 as f32 * dereth_terrain::land::drawn::BLOCK_METERS + p.frame.origin.x,
                at.1 as f32 * dereth_terrain::land::drawn::BLOCK_METERS + p.frame.origin.y,
            );
            placed.lifts = ground.lifts(x, y, p.frame.origin.z);
            placed
        })
        .collect();
    // The pack's ground cover, on full-detail ground.
    if let (false, Some(cover_ground)) = (covers.is_empty(), super::remaster_cover::ground_of(mesh))
    {
        let buildings: Vec<glam::Vec3> = placements
            .iter()
            .filter(|p| p.kind == rm::PlacementKind::Building)
            .map(|p| p.frame.w_axis.truncate())
            .collect();
        let inputs = CoverInputs {
            vertices: mesh.vertices.clone(),
            cuts: mesh.sw_to_ne_cut.clone(),
            water: mesh.cell_water.clone(),
            words: block.terrain,
            buildings,
            covers: covers.to_vec(),
            lifts: ground.lifts_key(at),
            knobs: ground.knobs().clone(),
        };
        let rows = super::remaster_cover::cover_rows(covers);
        let grown = Cover::grow(
            cache.entry(id),
            inputs,
            rows,
            allowance,
            |inputs, row, out| {
                super::remaster_cover::grow_row(
                    at,
                    &cover_ground,
                    &block.terrain,
                    covers,
                    &inputs.buildings,
                    &|x, y, z| ground.lifts(x, y, z),
                    row,
                    out,
                );
            },
        )?;
        placements.extend_from_slice(grown);
    }
    let b = rm::Block {
        block: id,
        generation: block.remaster_generation,
        terrain: rm::Terrain {
            side: mesh.side_cell_count,
            vertices: mesh.vertices.iter().copied().map(v3).collect(),
            cuts: mesh.sw_to_ne_cut.clone(),
            words: block.terrain,
        },
        placements,
        degrades: block_degrades(block).map(degrade).collect(),
        rooms,
        lights,
        lamps,
    };
    Some(b)
}

/// The lamps of `block`'s outdoor placements, block-local, each as high as its placement is drawn:
/// the baked placements and the animated ones drawn as live objects.
fn block_lamps(
    store: &RetailDatStore,
    at: (i32, i32),
    block: &BlockDraw,
    ground: &super::remaster_ground::RemasterGround,
) -> Vec<rm::Lamp> {
    use super::lamp_finder::{place_lamps, LampKind};
    let placements = block
        .remaster_placements
        .iter()
        .map(|p| (p.id, &p.frame, p.scale))
        .chain(
            block
                .emitters
                .iter()
                .filter(|e| e.animated)
                .map(|e| (e.setup, &e.frame, e.scale)),
        );
    place_lamps(store, placements)
        .into_iter()
        .map(|l| {
            let p = l.position;
            // LINT-OK: block coordinates, far inside f32's exact range.
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                at.0 as f32 * dereth_terrain::land::drawn::BLOCK_METERS + p.x,
                at.1 as f32 * dereth_terrain::land::drawn::BLOCK_METERS + p.y,
            );
            rm::Lamp {
                source: match l.source.kind {
                    LampKind::Authored => rm::LampSource::Authored,
                    LampKind::Flame => rm::LampSource::Flame,
                    LampKind::Glow => rm::LampSource::Glow,
                },
                flicker: l.source.flicker,
                // LINT-OK: hashing the bits of a position. Not a float conversion.
                seed: p.x.to_bits() ^ p.y.to_bits().rotate_left(11) ^ p.z.to_bits().rotate_left(22),
                position: p,
                lifts: ground.lifts(x, y, p.z),
                color: l.source.color,
                intensity: l.source.intensity,
                falloff: l.source.falloff,
            }
        })
        .collect()
}

/// The level each of `block`'s degrading placements draws at, and the ones turned toward the
/// viewer, with one number that changes when any of them does.
fn block_levels(id: u16, block: &BlockDraw) -> (rm::BlockLevels, u64) {
    use dereth_world_render::objects::degrade::DegradeMode;
    let mut levels = Vec::new();
    let mut turned = Vec::new();
    let mut key = 0xCBF2_9CE4_8422_2325u64;
    let mut mix = |v: u32| key = (key ^ u64::from(v)).wrapping_mul(0x0100_0000_01B3);
    for (i, p) in block_degrades(block).enumerate() {
        levels.push(level_byte(p.level));
        mix(p.level);
        if p.billboards && p.mode != DegradeMode::None {
            let index = u32::try_from(i).unwrap_or(u32::MAX);
            mix(index);
            for v in [
                p.draw_pos.origin.x,
                p.draw_pos.origin.y,
                p.draw_pos.origin.z,
                p.draw_pos.rotation.w,
                p.draw_pos.rotation.x,
                p.draw_pos.rotation.y,
                p.draw_pos.rotation.z,
            ] {
                mix(v.to_bits());
            }
            turned.push((index, mat(&p.draw_pos)));
        }
    }
    (
        rm::BlockLevels {
            block: id,
            levels,
            turned,
        },
        key,
    )
}

/// One part of a moving object as this frame poses it.
struct Posed {
    /// Where its mesh stands, its scale applied first.
    transform: glam::Mat4,
    level: u32,
    hidden: bool,
    material: Option<rm::Material>,
}

/// One moving object as the bridge reads it: its key, what it is built from, the object as the
/// remaster is sent it when that differs from what it was last sent, its parts as posed this
/// frame, and its cell.
struct Moving {
    key: rm::ObjectKey,
    build: u64,
    object: Option<rm::Object>,
    parts: Vec<Posed>,
    cell: u32,
    /// What holds it, and the holder's part it hangs on.
    held_by: Option<(rm::ObjectKey, u16)>,
    /// The animation its parts are playing.
    motion: Option<rm::Motion>,
}

/// What `meshes` draws at each level of each part, with the looks of `parts`.
fn object_parts(
    meshes: &[PartLevels],
    parts: &[dereth_animation::parts::PhysicsPart],
) -> Vec<rm::ObjectPart> {
    meshes
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let levels = match &pl.info {
                Some(info) => info.degrades.iter().map(|e| e.gfxobj_id.0).collect(),
                None if pl.levels.first().is_some_and(|l| !l.is_empty()) => vec![pl.gfxobj.0],
                None => Vec::new(),
            };
            let look = parts
                .get(i)
                .and_then(|p| p.surface_overrides.as_ref())
                .map(|o| rm::PartLook {
                    textures: o.texture_maps.iter().map(|(a, b)| (a.0, b.0)).collect(),
                    palette: o.shift_palette.map(|p| p.0),
                    ranges: o
                        .subpalettes
                        .iter()
                        .map(|r| (r.palette_set.0, r.offset, r.length))
                        .collect(),
                });
            rm::ObjectPart { levels, look }
        })
        .collect()
}

/// How fast, and which way, the cycle `seq` repeats asks to carry its object, in the object's own
/// space before its scale, meters a second ([`rm::Motion::velocity`]): the ground each of its
/// animation's frames moves the object on, at the rate it plays them, with the motion's own
/// steady speed. It is what the object would cover were nothing in its way: a creature pressed
/// against a crowd still asks it.
pub(super) fn cycle_velocity(seq: &dereth_animation::seq::Sequence) -> glam::Vec3 {
    seq.first_cyclic()
        .map_or_else(|| v3(seq.velocity), |c| node_velocity(seq, c, None))
}

/// How fast, and which way, the node `node` of `seq` carries its object, as [`cycle_velocity`]
/// reads a cycle's: over all its frames, or at the frame `at` alone (a link whose steps carry the
/// object on a stopping stride, then stand).
pub(super) fn node_velocity(
    seq: &dereth_animation::seq::Sequence,
    node: usize,
    at: Option<f64>,
) -> glam::Vec3 {
    let steady = v3(seq.velocity);
    let Some(node) = seq.nodes().get(node) else {
        return steady;
    };
    let Some(steps) = node.anim.pos_frames.as_ref() else {
        return steady;
    };
    let (low, high) = match at {
        Some(f) => {
            let f = dereth_primitives::num::to_i32_f64(f.floor())
                .clamp(node.low_frame, node.high_frame.max(node.low_frame));
            (f, f)
        }
        None => (node.low_frame, node.high_frame),
    };
    let frames = || (low..=high).filter_map(|i| steps.get(usize::try_from(i).ok()?));
    let count = frames().count();
    if count == 0 {
        return steady;
    }
    let sum = frames().fold(glam::Vec3::ZERO, |a, f| a + v3(f.origin));
    #[allow(clippy::cast_precision_loss)] // an animation's frames
    let each = sum / count as f32;
    steady + each * node.framerate
}

/// The node of `seq` and the frame of it, keyframes and the share of one between them, a body
/// whose parts are placed `ahead` seconds on is drawn at, as its parts are drawn: between the
/// keyframes, while they are drawn between them; at the keyframe the parts stand at, with `None`.
/// `None` with no animation.
pub(super) fn drawn_at(
    seq: &dereth_animation::seq::Sequence,
    ahead: Option<f64>,
) -> Option<(usize, f64)> {
    let curr = seq.curr()?;
    let Some(ahead) = ahead else {
        return Some((curr, seq.frame_number().floor()));
    };
    let b = seq.between(ahead)?;
    // Toward the next keyframe in the node, or past its last toward the next node's first.
    let step = if b.to.0 == b.from.0 {
        f64::from(b.to.1 - b.from.1)
    } else {
        f64::from(seq.nodes().get(b.from.0)?.framerate.signum())
    };
    Some((b.from.0, f64::from(b.from.1) + step * f64::from(b.share)))
}

/// The object `key` on `setup`, built as `build`, laying the palettes `palettes`, its parts'
/// meshes `meshes`, posed from `driver`'s parts with each part's level and draw frame from
/// `levels` and `draw_pos`, standing in `cell` and in the world when `drawn`, drawn `lift` meters
/// above where it stands; the object itself is made only when `sent` has another build.
#[allow(clippy::too_many_arguments)] // one parameter per input the reading takes
fn read_moving(
    (key, setup, build, palettes, who): (rm::ObjectKey, u32, u64, Vec<u32>, rm::Who),
    sent: &HashMap<rm::ObjectKey, u64>,
    meshes: &[PartLevels],
    driver: &dereth_animation::driver::MotionDriver,
    (levels, draw_pos): (&[u32], &[Frame]),
    cell: u32,
    (drawn, lift): (bool, f32),
) -> Moving {
    let parts = &driver.part_array.parts[..];
    let object = (sent.get(&key) != Some(&build)).then(|| rm::Object {
        key,
        setup,
        parts: object_parts(meshes, parts),
        palettes,
        who,
    });
    let posed = (0..meshes.len())
        .map(|i| {
            let part = parts.get(i);
            let pos = draw_pos
                .get(i)
                .copied()
                .or_else(|| part.map(|p| p.pos))
                .unwrap_or_default();
            let scale = part.map_or(Vec3::new(1.0, 1.0, 1.0), |p| p.gfxobj_scale);
            Posed {
                transform: glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, lift))
                    * mat(&pos)
                    * glam::Mat4::from_scale(v3(scale)),
                level: levels.get(i).copied().unwrap_or(0),
                hidden: !drawn || part.is_none_or(dereth_animation::parts::PhysicsPart::no_draw),
                material: material(part.and_then(|p| p.material)),
            }
        })
        .collect();
    Moving {
        key,
        build,
        object,
        parts: posed,
        cell,
        held_by: None,
        motion: super::remaster_slots::motion_of(driver, lift),
    }
}

/// Every moving object of the scene, in the scene's order: the server's, then the body, then each
/// block's animated statics; `sent` is what the receiver was last sent of them. What stands
/// outdoors is drawn on the remaster's ground: an animated static on the ground under it as
/// `nears` keeps it, by its key.
fn moving(
    draw: &SceneDraw,
    ws: &WorldState,
    sent: &HashMap<rm::ObjectKey, u64>,
    nears: &mut HashMap<rm::ObjectKey, (u64, u32, f32)>,
) -> Vec<Moving> {
    use dereth_physics::landdefs::is_outdoors;
    let mut out = Vec::new();
    // What holds each held thing: the holder (the body when it is the player), and the holder's
    // part its holding location hangs on.
    let mut held_by: HashMap<dereth_primitives::ObjectId, (rm::ObjectKey, u16)> = HashMap::new();
    // The body is the player's: the id the server named it by.
    let player = ws
        .character
        .as_ref()
        .map(|c| ws.player_object_id().unwrap_or(c.object_id()));
    for (id, o) in &ws.objects {
        let Some((holder, location)) = o.sim.parent else {
            continue;
        };
        let entry = if player == Some(holder) {
            ws.character.as_ref().and_then(|c| {
                c.driver()
                    .part_array
                    .setup
                    .as_ref()
                    .and_then(|s| s.holding_locations.get(&location).map(|l| l.part_id))
            })
        } else {
            ws.objects.get(&holder).and_then(|h| {
                h.sim
                    .driver
                    .borrow()
                    .part_array
                    .setup
                    .as_ref()
                    .and_then(|s| s.holding_locations.get(&location).map(|l| l.part_id))
            })
        };
        if let Some(part) = entry.and_then(|p| u16::try_from(p).ok()) {
            let key = if player == Some(holder) {
                rm::ObjectKey::Body
            } else {
                rm::ObjectKey::Server(holder.0)
            };
            held_by.insert(*id, (key, part));
        }
    }
    for (id, d) in &draw.object_draws {
        let Some(o) = ws.objects.get(id) else {
            continue;
        };
        let driver = o.sim.driver.borrow();
        let who = rm::Who {
            weenie: o.weenie,
            launcher: o.held_launcher,
        };
        out.push(read_moving(
            (
                rm::ObjectKey::Server(id.0),
                d.setup.0,
                // Who it is decides whether a pack draws its body for it: a change sends it again.
                build_of(d.setup, d.appearance.as_ref(), &d.meshes, 0)
                    ^ u64::from(who.launcher) << 63,
                driver
                    .part_array
                    .described_palettes
                    .iter()
                    .map(|p| p.0)
                    .collect(),
                who,
            ),
            sent,
            &d.meshes,
            &driver,
            (&d.part_levels, &d.part_draw_pos),
            ws.object_draw_cell(o).map_or(0, |c| c.0),
            (o.drawn, draw.remaster_ground.object_lift(ws, o)),
        ));
        if let Some(m) = out.last_mut() {
            m.held_by = held_by.get(id).copied();
        }
    }
    if let Some(c) = ws.character.as_ref() {
        if !draw.character_parts.is_empty() {
            let driver = c.driver();
            out.push(read_moving(
                (
                    rm::ObjectKey::Body,
                    c.setup_id().0,
                    build_of(
                        c.setup_id(),
                        None,
                        &draw.character_parts,
                        draw.remaster_body_generation,
                    ),
                    driver
                        .part_array
                        .described_palettes
                        .iter()
                        .map(|p| p.0)
                        .collect(),
                    rm::Who::default(),
                ),
                sent,
                &draw.character_parts,
                &driver,
                (&draw.character_part_levels, &draw.character_part_draw_pos),
                c.position().cell.0,
                (true, draw.remaster_ground.body_lift(c)),
            ));
        }
    }
    let live_hosts = draw.live_hosts(ws);
    for (key, block) in &draw.blocks {
        if !live_hosts.live(*key) {
            continue;
        }
        for h in &block.hosts {
            let Some(meshes) = h.meshes.as_deref() else {
                continue;
            };
            let setup = block
                .emitters
                .get(h.placement)
                .map_or(DataId(0), |e| e.setup);
            let key = animated(h.cell.0, h.placement);
            // The host stands in the world's own coordinates, and stays where it stands: the
            // ground under it is worked out once and kept.
            let at = Vec3::new(
                h.frame.origin.x - draw.remaster_ground.global.0,
                h.frame.origin.y - draw.remaster_ground.global.1,
                h.frame.origin.z,
            );
            let lift = if is_outdoors(h.cell) {
                let near = draw.remaster_ground.near_key(at);
                let mut kept = nears.get(&key).copied();
                let lift = draw.remaster_ground.still_shift_at(at, near, &mut kept);
                if let Some(k) = kept {
                    nears.insert(key, k);
                }
                lift
            } else {
                0.0
            };
            out.push(read_moving(
                (
                    key,
                    setup.0,
                    build_of(setup, None, meshes, block.remaster_generation),
                    h.driver
                        .part_array
                        .described_palettes
                        .iter()
                        .map(|p| p.0)
                        .collect(),
                    rm::Who::default(),
                ),
                sent,
                meshes,
                &h.driver,
                (&h.part_levels, &h.part_draw_pos),
                h.cell.0,
                (true, lift),
            ));
        }
    }
    out
}

/// Hand the receiver in `bridge` the changes since it was last published to and the state of the
/// frame whose world was just walked, at a back buffer of `size`. Nothing is built when no
/// receiver is installed.
#[allow(clippy::too_many_lines)]
pub(super) fn publish(draw: &SceneDraw, bridge: &mut Bridge, ws: &WorldState, size: (u32, u32)) {
    let Some(receiver) = bridge.receiver.as_deref_mut() else {
        return;
    };
    let sh = &mut bridge.shadow;
    sh.serial += 1;
    let mut deltas = Vec::new();
    let mut frame = rm::FrameDelta {
        serial: sh.serial,
        ..rm::FrameDelta::default()
    };

    // --- the origin, and what moves with it --------------------------------------------
    let window = &ws.streamer.window;
    let origin = window.viewer_block().map_or_else(
        || {
            let (x, y) = dereth_world_data::landblock::block_xy(draw.cfg.landblock);
            [x, y]
        },
        |(x, y)| [x, y],
    );
    // A point that stood at `p` in the last frame's space stands at `p - moved` in this one's.
    #[allow(clippy::cast_precision_loss)] // a few blocks, at most the whole map
    let moved = sh.origin.map_or(glam::Vec3::ZERO, |last| {
        glam::Vec3::new(
            (origin[0] - last[0]) as f32 * dereth_terrain::consts::BLOCK_LENGTH,
            (origin[1] - last[1]) as f32 * dereth_terrain::consts::BLOCK_LENGTH,
            0.0,
        )
    });
    sh.origin = Some(origin);
    frame.origin = origin;

    // --- the camera -----------------------------------------------------------------------
    let (width, height) = size;
    let view = draw.view_params(ws, width, height);
    let projection = dereth_render::camera::projection(&view);
    let to_eye = view.view * swap_zup_to_d3d();
    let view_projection = projection * to_eye;
    let (near, far) = planes(projection);
    frame.camera = rm::Camera {
        view: to_eye,
        projection,
        previous_view_projection: sh
            .previous_view_projection
            .map(|vp| vp * glam::Mat4::from_translation(moved)),
        eye: v3(draw.drawn_eye(ws).origin),
        viewport: [
            view.viewport.x,
            view.viewport.y,
            view.viewport.width,
            view.viewport.height,
        ],
        fov_y: view.fov_y_rad,
        aspect: view.aspect,
        near,
        far,
    };
    sh.previous_view_projection = Some(view_projection);

    // --- the light inputs -----------------------------------------------------------------
    let lighting = &draw.land.lighting;
    let fog = draw.world_fog_state(ws);
    let ambient = dereth_world_render::lighting::ambient_render_state(draw.world_ambient_color(ws));
    let [_, ar, ag, ab] = ambient.to_be_bytes();
    let pools = draw.light_pools();
    let pool_light = |l: &dereth_world_render::lighting::RenderLight| rm::Light {
        kind: light_kind(l.info.light_type),
        cell: l.cell_id.0,
        position: v3(l.info.offset.origin),
        direction: v3(dereth_world_render::lighting::get_direction(&l.info)),
        color: l.info.color,
        intensity: l.info.intensity,
        falloff: l.info.falloff,
        cone_angle: l.info.cone_angle,
    };
    frame.light = rm::RetailLight {
        sun: v3(lighting.sunlight),
        sun_color: lighting.sunlight_color,
        ambient_level: lighting.ambient_level,
        ambient_color: lighting.ambient_color,
        object_ambient: [ar, ag, ab].map(|c| f32::from(c) / 255.0),
        fog: fog.enabled.then(|| {
            let [_, r, g, b] = fog.color.to_be_bytes();
            rm::Fog {
                start: fog.near,
                end: fog.far,
                color: [r, g, b],
            }
        }),
        outdoor: ws.viewer_cell().is_none(),
        sees_outside: ws
            .viewer_cell()
            .is_none_or(|cell| ws.cell_seen_outside(cell)),
        time_of_day: ws.clock.present_time_of_day,
        lights: pools
            .statics
            .iter()
            .chain(&pools.dynamics)
            .map(pool_light)
            .collect(),
        statics: pools.statics.len(),
    };

    // --- the sky --------------------------------------------------------------------------
    if let Some(sky) = draw.sky.as_ref() {
        let views: Vec<_> = sky.views().collect();
        frame.sky = sky
            .drawn(ws.camera.position, ws.viewer_cell().is_none())
            .map(|o| rm::SkyObject {
                deck: views
                    .iter()
                    .find(|v| v.gfx == o.gfx)
                    .and_then(|v| v.deck)
                    .map(|d| rm::SkyDeck {
                        laying: d.laying,
                        height: d.height,
                    }),
                gfx: o.gfx.0,
                properties: o.properties,
                frame: mat(&o.frame),
                before: o.before,
                after: o.after,
                material: material(o.material),
                uv_offset: [o.uv_offset.0, o.uv_offset.1],
                fogged: sky.override_enabled,
            })
            .collect();
    }

    // --- the blocks -----------------------------------------------------------------------
    // The pack's ground cover whose kind is switched on.
    let covers: Vec<dereth_assets::art_pack::ManifestCover> = {
        let view = draw.remaster_view.borrow();
        view.pack
            .as_ref()
            .map(|p| {
                p.pack()
                    .covers
                    .iter()
                    .filter(|c| p.model(c.id.0, view.packs).is_some())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut cover_budget = CoverBudget::after(sh.cover_whole);
    let mid = window.mid_width();
    let mut resident: Vec<u16> = Vec::new();
    for xi in 0..mid {
        for yi in 0..mid {
            let Some(slot) = window.slot(xi, yi) else {
                continue;
            };
            let Some(block) = draw.blocks.get(&(slot.block_x, slot.block_y)) else {
                continue;
            };
            let Some(mesh) = slot.mesh.as_ref().and_then(
                dereth_client_runtime::world_stream::SlotMesh::get::<
                    dereth_terrain::land::mesh::LandblockMesh,
                >,
            ) else {
                continue;
            };
            let id = block_of((slot.block_x, slot.block_y));
            resident.push(id);
            let sent = sh.blocks.get(&id).copied();
            let at = (slot.block_x, slot.block_y);
            // Ground cover grows on the viewer's block and those round it alone: a block is sent
            // again as it comes into that ring or leaves it.
            let near = (at.0 - origin[0]).abs() <= 1 && (at.1 - origin[1]).abs() <= 1;
            let lifts_key = draw.remaster_ground.lifts_key(at)
                ^ if near && !covers.is_empty() {
                    0x00C0_7E20_0000_0000
                } else {
                    0
                };
            if sent != Some(block.remaster_generation)
                || sh.shifts.get(&id).copied().unwrap_or(0) != lifts_key
            {
                // A block coming in grows its cover at once. While the landscape streams within a
                // budget, one sent before grows its cover a share a frame, and is drawn as it
                // was until the cover is whole.
                let bounded = sent.is_some() && draw.cfg.stream_budget.is_some();
                let Some(b) = block_delta(
                    (id, at),
                    block,
                    mesh,
                    &draw.remaster_ground,
                    draw.remaster_view.borrow().store.as_ref(),
                    (
                        if near { &covers } else { &[] },
                        bounded.then_some(&mut cover_budget),
                    ),
                    &mut sh.covers,
                ) else {
                    continue;
                };
                let b = Box::new(b);
                sh.shifts.insert(id, lifts_key);
                let last = sent.and_then(|_| sh.contents.get_mut(&id));
                let same = last.as_deref().is_some_and(|l| same_contents(l, &b));
                let unchanged = same && last.as_deref().is_some_and(|l| l.terrain == b.terrain);
                if unchanged {
                    // Built again with nothing the remaster draws changed (its lighting, say):
                    // nothing is sent.
                } else if same {
                    if let Some(l) = last {
                        l.terrain = b.terrain.clone();
                    }
                    // Built again for its ground alone (its level of detail or its stitching):
                    // the parts the remaster holds stay, and so do their levels.
                    deltas.push(rm::SceneDelta::GroundChanged(Box::new(rm::GroundDelta {
                        block: id,
                        generation: b.generation,
                        terrain: b.terrain,
                    })));
                } else {
                    sh.contents.insert(id, (*b).clone());
                    deltas.push(if sent.is_some() {
                        rm::SceneDelta::BlockChanged(b)
                    } else {
                        rm::SceneDelta::BlockIn(b)
                    });
                    sh.levels.remove(&id);
                }
                sh.blocks.insert(id, block.remaster_generation);
            }
            // Read again only when the walk has chosen another level or turn for one of them.
            if sh.level_serials.get(&id) != Some(&block.levels_serial)
                || !sh.levels.contains_key(&id)
            {
                let (levels, key) = block_levels(id, block);
                if sh.levels.get(&id) != Some(&key) {
                    if !levels.levels.is_empty() || sh.levels.contains_key(&id) {
                        frame.levels.push(levels);
                    }
                    sh.levels.insert(id, key);
                }
                sh.level_serials.insert(id, block.levels_serial);
            }
        }
    }
    let mut gone: Vec<u16> = sh
        .blocks
        .keys()
        .copied()
        .filter(|id| !resident.contains(id))
        .collect();
    gone.sort_unstable();
    for id in gone {
        deltas.push(rm::SceneDelta::BlockOut(id));
        sh.blocks.remove(&id);
        sh.levels.remove(&id);
        sh.level_serials.remove(&id);
        sh.contents.remove(&id);
        sh.shifts.remove(&id);
        sh.covers.remove(&id);
    }
    sh.cover_whole = cover_budget.whole;

    // --- the moving objects ---------------------------------------------------------------
    let objects = moving(draw, ws, &sh.objects, &mut sh.host_nears);
    let present: std::collections::HashSet<rm::ObjectKey> = objects.iter().map(|o| o.key).collect();
    let mut gone: Vec<rm::ObjectKey> = sh
        .objects
        .keys()
        .copied()
        .filter(|k| !present.contains(k))
        .collect();
    gone.sort_unstable();
    for key in gone {
        deltas.push(rm::SceneDelta::ObjectRemoved(key));
        sh.objects.remove(&key);
        sh.previous.remove(&key);
        sh.host_nears.remove(&key);
    }
    for o in objects {
        if let Some(object) = o.object {
            let object = Box::new(object);
            deltas.push(if sh.objects.contains_key(&o.key) {
                rm::SceneDelta::ObjectChanged(object)
            } else {
                rm::SceneDelta::ObjectBuilt(object)
            });
            sh.objects.insert(o.key, o.build);
            sh.previous.remove(&o.key);
        }
        let last = sh.previous.get(&o.key);
        let poses: Vec<rm::PartPose> = o
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| rm::PartPose {
                transform: p.transform,
                previous: last
                    .and_then(|l| l.get(i))
                    .map_or(p.transform, |m| glam::Mat4::from_translation(-moved) * *m),
                level: level_byte(p.level),
                hidden: p.hidden,
                material: p.material,
            })
            .collect();
        // Kept in place: the same parts are posed frame after frame.
        let transforms = o.parts.iter().map(|p| p.transform);
        match sh.previous.get_mut(&o.key) {
            Some(kept) => {
                kept.clear();
                kept.extend(transforms);
            }
            None => {
                sh.previous.insert(o.key, transforms.collect());
            }
        }
        frame.objects.push(rm::ObjectPose {
            key: o.key,
            cell: o.cell,
            parts: poses,
            held_by: o.held_by,
            motion: o.motion,
        });
    }

    // --- what the walk drew ---------------------------------------------------------------
    frame.drawn_cells = draw
        .frame_drawn_cells
        .borrow()
        .as_ref()
        .map(|c| c.iter().copied().collect())
        .unwrap_or_default();
    let first = draw.frame_stamp_first.get();
    for block in draw.blocks.values() {
        for c in &block.env_cells {
            for (k, p) in c.objects.parts.iter().enumerate() {
                let (stamp, _, _) = p.drawn.get();
                if stamp != 0 && stamp.wrapping_sub(first) < 0x8000_0000 {
                    frame
                        .drawn_cell_parts
                        .push((c.id.0, u32::try_from(k).unwrap_or(u32::MAX)));
                }
            }
        }
    }
    if let Some(log) = draw.frame_walk.borrow().as_ref() {
        frame.landscape = log.landscape;
        frame.drawn_blocks.clone_from(&log.blocks);
        let statics = pools.statics.len();
        frame.drawn_parts = log
            .parts
            .iter()
            .map(|w| {
                let mut lights = [0u16; 8];
                let mut count = 0u8;
                if !w.outdoors && draw.cfg.object_lighting {
                    let active = dereth_world_render::lighting::minimize_object_lighting(
                        pools, w.centre, w.radius,
                    );
                    for slot in &active.cur {
                        let Ok(i) = usize::try_from(slot.index) else {
                            continue;
                        };
                        let index = match slot.light_class {
                            1 => i,
                            2 => statics + i,
                            _ => continue,
                        };
                        let at = usize::from(count);
                        if let (true, Ok(index)) = (at < lights.len(), u16::try_from(index)) {
                            lights[at] = index;
                            count += 1;
                        }
                    }
                }
                rm::DrawnPart {
                    key: w.key,
                    part: w.part,
                    outdoors: w.outdoors,
                    lights,
                    count,
                }
            })
            .collect();
        // Outdoors, a particle stands on the ground as the remaster draws it, as the bodies do.
        let lifted = draw.remaster_ground.active;
        frame.particles = log
            .particles
            .iter()
            .map(|w| {
                let mut d = w.drawn;
                if lifted && d.outdoors {
                    let lift = draw.remaster_ground.shift_at(d.frame.origin);
                    d.frame.origin.z += lift;
                    d.centre.z += lift;
                }
                let d = &d;
                let (lights, count) = if !d.outdoors && draw.cfg.object_lighting {
                    light_indices(pools, d.centre, d.radius)
                } else {
                    ([0u16; 8], 0)
                };
                rm::ParticleDraw {
                    gfx: w.gfx,
                    transform: mat(&d.frame) * glam::Mat4::from_scale(glam::Vec3::splat(d.scale)),
                    material: rm::Material {
                        translucency: d.translucency,
                        diffusion: crate::particles::PARTICLE_MATERIAL_LIGHTING[1],
                        luminosity: crate::particles::PARTICLE_MATERIAL_LIGHTING[0],
                    },
                    outdoors: d.outdoors,
                    lit: draw.cfg.object_lighting,
                    lights,
                    count,
                }
            })
            .collect();
    }

    // The day group the sky follows today, which the remaster's look reads its kind of day from.
    frame.day_group = dereth_world_render::sky::drawn_day_group(
        draw.sky_region(),
        ws.clock.current_year,
        ws.clock.current_day,
        super::lighting::asked_day(ws),
    )
    .map(|g| g.day_name.clone())
    .unwrap_or_default();

    // The game's weather: whether it is drawn, and the form the player asked it to fall in.
    frame.weather = rm::FrameWeather {
        enabled: draw.weather_enabled().unwrap_or(false),
        asked: match ws.weather {
            dereth_client_runtime::weather::Weather::Rain => rm::AskedFall::Rain,
            dereth_client_runtime::weather::Weather::Snow => rm::AskedFall::Snow,
            dereth_client_runtime::weather::Weather::Auto
            | dereth_client_runtime::weather::Weather::Clear => rm::AskedFall::Land,
        },
    };

    // --- the moving objects' lights outdoors ---------------------------------------------
    frame.lamps = outdoor_lamps(draw, ws);
    frame.clock = sh.now;
    frame.teleports = ws.character.as_ref().map_or(0, |c| {
        c.stats.teleports_committed + c.stats.teleports_uncommitted
    });
    let g = draw.degrade_globals(ws);
    frame.degrade = rm::DegradeBias {
        bias: if g.auto_update_deg_mul {
            g.deg_mul
        } else {
            g.user_bias
        },
        distance: g.degrade_distance,
        disabled: g.degrades_disabled,
        force: g.force_level,
    };
    frame.scenery_reach = scenery_reach(draw.cfg.object_rings());

    // --- the ground drawn over retail's ---------------------------------------------------
    frame.drawn = drawn_ground(draw, ws, &mut sh.drawn_world);
    frame.forest = forest(draw, sh);

    receiver.receive(deltas, frame);
}

/// The ground the remaster draws over retail's, while it draws its grid: the world's vertices
/// (sent anew only when read anew, `world` keeping the last), and while the scene draws its offset
/// the offset round the viewer, its knobs and the bodies the fine relief is leveled under.
/// One tree far off, as the remaster draws it.
/// How far the eye may stand from the viewer's body, meters, as the camera follows it.
const CAMERA_REACH: f32 = 24.0;

/// How far from the eye the blocks whose own scenery stands in the world reach at the least, with
/// `radius` rings of blocks round the viewer's growing it ([`SceneConfig::object_rings`]): a block
/// leaves them, or comes into them, only once the viewer stands a whole ring of blocks from it.
/// 0 for none.
///
/// [`SceneConfig::object_rings`]: dereth_client_runtime::scene::SceneConfig::object_rings
fn scenery_reach(radius: u32) -> f32 {
    // LINT-OK: a radius of a few blocks.
    #[allow(clippy::cast_precision_loss)]
    let r = radius as f32;
    (r * 192.0 - CAMERA_REACH).max(0.0)
}

pub(crate) type FarTree = rm::FarTree;

impl super::far_trees::Tree for rm::FarTree {
    fn placed(at: glam::Vec3, rotation: glam::Quat, scale: f32, id: u32, seed: u32) -> Self {
        Self {
            at,
            rotation,
            scale,
            id,
            seed,
        }
    }
}

/// How the far trees and the forest cover are drawn, from the knobs.
fn far_knobs(k: &dereth_terrain::land::drawn::Knobs) -> rm::FarKnobs {
    use dereth_terrain::land::drawn::Knob;
    rm::FarKnobs {
        trees: k.on(Knob::FarTrees),
        reach: k.get(Knob::TreeReach),
        thin_from: k.get(Knob::TreeThinFrom),
        widen: k.get(Knob::TreeWiden),
        soften: k.get(Knob::TreeSoften),
        skyline: k.get(Knob::TreeSkyline),
        cover: k.get(Knob::ForestCover),
        canopy: k.get(Knob::ForestCanopy),
    }
}

/// The far forest as the remaster draws it, while the scene works it out: retail's own trees and
/// an art pack's whose trees are drawn; what is handed over is made again only when it changes.
fn forest(draw: &SceneDraw, sh: &mut Shadow) -> Option<rm::Forest> {
    let far = &draw.remaster_far;
    let view = draw.remaster_view.borrow();
    let pack = view.pack.as_ref().filter(|_| {
        view.packs
            .models(dereth_remaster::assets::pack::Category::Tree)
    });
    let retail = draw.remaster_retail_far.as_ref();
    let retail_models = retail.and_then(|r| r.models()).map(|m| m.len());
    if !(far.active || far.cover.is_some()) || (pack.is_none() && retail_models.is_none()) {
        sh.far_pack = None;
        sh.far_trees = None;
        sh.far_cover = None;
        return None;
    }
    // The objects the pack draws its own models for near, whatever their kind, and which kinds.
    let kinds = dereth_remaster::assets::pack::Category::ALL
        .iter()
        .enumerate()
        .filter(|(_, c)| view.packs.models(**c))
        .fold(0u64, |bits, (k, _)| bits | 1 << k);
    let serial = super::far_retail::set_serial(pack.map(|p| p.pack().serial), retail_models)
        ^ view
            .pack
            .as_ref()
            .map_or(0, |p| p.pack().serial.rotate_left(29) ^ kinds << 56);
    if sh.far_pack.as_ref().is_none_or(|p| p.serial != serial) {
        let swapped = view
            .pack
            .as_ref()
            .map(|p| p.swapped(view.packs))
            .unwrap_or_default();
        sh.far_pack =
            super::far_retail::far_pack(pack.map(|p| p.far_pack()), retail, &swapped, serial)
                .map(std::sync::Arc::new);
    }
    if sh.far_trees.as_ref().is_none_or(|t| t.serial != far.serial) {
        let blocks = far
            .within()
            .map(|((bx, by), list)| {
                // LINT-OK: block indices are 0..=0xFE.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let id = rm::block_id(bx as u8, by as u8);
                (id, std::sync::Arc::clone(list))
            })
            .collect();
        sh.far_trees = Some(std::sync::Arc::new(rm::FarTreeSet {
            serial: far.serial,
            blocks,
        }));
    }
    match far.cover.as_ref() {
        Some((serial, made)) => {
            if sh.far_cover.as_ref().is_none_or(|c| c.serial != *serial) {
                sh.far_cover = Some(std::sync::Arc::new(rm::ForestCover {
                    serial: *serial,
                    // LINT-OK: 2041.
                    #[allow(clippy::cast_possible_truncation)]
                    side: super::far_trees::WORLD_SIDE as u32,
                    color: std::sync::Arc::new(made.colour.clone()),
                    canopy: std::sync::Arc::new(made.canopy.clone()),
                }));
            }
        }
        None => sh.far_cover = None,
    }
    let mut knobs = far_knobs(draw.remaster_ground.knobs());
    knobs.trees &= far.active;
    Some(rm::Forest {
        pack: std::sync::Arc::clone(sh.far_pack.as_ref()?),
        trees: std::sync::Arc::clone(sh.far_trees.as_ref()?),
        cover: sh.far_cover.clone(),
        knobs,
    })
}

fn drawn_ground(
    draw: &SceneDraw,
    ws: &WorldState,
    world: &mut Option<(u64, std::sync::Arc<rm::DrawnWorld>)>,
) -> Option<rm::DrawnGround> {
    let (grid, _, _) = draw.remaster_ground_wanted();
    if !grid {
        return None;
    }
    let d = &draw.remaster_ground;
    let (vertices, serial) = d.world()?;
    if world.as_ref().is_none_or(|(s, _)| *s != serial) {
        *world = Some((
            serial,
            std::sync::Arc::new(rm::DrawnWorld {
                serial,
                // LINT-OK: 2041.
                #[allow(clippy::cast_possible_truncation)]
                side: super::remaster_ground::WORLD_SIDE as u32,
                heights: std::sync::Arc::clone(&vertices.heights),
                words: std::sync::Arc::clone(&vertices.words),
                water: std::sync::Arc::clone(&vertices.water),
            }),
        ));
    }
    let mut out = rm::DrawnGround {
        world: std::sync::Arc::clone(&world.as_ref()?.1),
        global: glam::Vec2::new(d.global.0, d.global.1),
        ..rm::DrawnGround::default()
    };
    if !d.active {
        return Some(out);
    }
    out.reach = d.reach();
    out.tiles = d.near_tiles();
    out.knobs = d.drawn_knobs();
    // The fine relief is leveled under the bodies standing near the eye: the player's and the
    // others', never a held thing or a missile in flight.
    let eye = d.eye;
    let near = |p: &Vec3| {
        (p.x - eye.x).powi(2) + (p.y - eye.y).powi(2)
            < super::remaster_ground::LEVELED_WITHIN.powi(2)
    };
    out.bodies = ws
        .character
        .as_ref()
        .map(|c| c.render_frame().origin)
        .into_iter()
        .chain(
            ws.objects
                .values()
                .filter(|o| o.drawn && o.sim.parent.is_none() && !o.sim.state.is_missile())
                .map(|o| o.frame.origin),
        )
        .filter(near)
        .take(super::remaster_ground::LEVELED_BODIES)
        .map(|p| glam::Vec3::new(p.x, p.y, p.z))
        .collect();
    Some(out)
}

/// The lights of the moving objects that stand outdoors with their light on, in the frame's
/// space: the server's objects and the body, as the light pools take them indoors.
fn outdoor_lamps(draw: &SceneDraw, ws: &WorldState) -> Vec<rm::Light> {
    use dereth_physics::landdefs::is_outdoors;
    let mut out = Vec::new();
    let mut push = |info: &dereth_world_render::lighting::LightInfo, cell: u32, at: &Frame| {
        let placed = dereth_world_render::lighting::LightInfo {
            offset: dereth_terrain::math::combine(at, &info.offset),
            ..*info
        };
        out.push(rm::Light {
            kind: light_kind(info.light_type),
            cell,
            position: v3(placed.offset.origin),
            direction: v3(dereth_world_render::lighting::get_direction(&placed)),
            color: info.color,
            intensity: info.intensity,
            falloff: info.falloff,
            cone_angle: info.cone_angle,
        });
    };
    for o in ws.objects.values() {
        let Some(pos) = o.sim.position else {
            continue;
        };
        if !o.drawn
            || !is_outdoors(pos.cell)
            || !o.sim.state.is_lighting_on()
            || o.sim.state.is_static()
        {
            continue;
        }
        let driver = o.sim.driver.borrow();
        let Some(setup) = driver.part_array.setup.as_ref() else {
            continue;
        };
        for info in super::setup_data_lights(setup) {
            push(&info, pos.cell.0, &o.frame);
        }
    }
    if let Some(c) = ws.character.as_ref() {
        let pos = c.position();
        if is_outdoors(pos.cell) && ws.character_state.is_lighting_on() {
            let at = draw.render_frame_of(ws, pos);
            let driver = c.driver();
            if let Some(setup) = driver.part_array.setup.as_ref() {
                for info in super::setup_data_lights(setup) {
                    push(&info, pos.cell.0, &at);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A sequence of one ten-keyframe animation at thirty a second, `seconds` into it.
    fn ten_keyframes(seconds: f64) -> dereth_animation::seq::Sequence {
        use dereth_animation::data::{AnimData, AnimFrame, AnimationData, MapAssets};
        let id = DataId(0x0300_0001);
        let mut assets = MapAssets::default();
        assets.animations.insert(
            id.0,
            std::sync::Arc::new(AnimationData {
                num_frames: 10,
                num_parts: 1,
                pos_frames: None,
                part_frames: (0..10)
                    .map(|_| AnimFrame {
                        frames: vec![Frame::default()],
                        hooks: Vec::new(),
                    })
                    .collect(),
                has_hooks: false,
            }),
        );
        let mut s = dereth_animation::seq::Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            },
            &assets,
        );
        s.update(seconds, Some(&mut Frame::default()), &mut Vec::new());
        s
    }

    /// Drawn at keyframes (the Smooth animations box off), a body is drawn at the keyframe its
    /// sequence stands at; drawn between them, where its parts are drawn: the frames it has
    /// gone into the next keyframe, and on as far as its parts are drawn ahead.
    #[test]
    fn a_body_is_drawn_at_its_keyframe_or_between_them_as_its_parts_are() {
        let s = ten_keyframes(3.5 / 30.0);
        let near = |a: Option<(usize, f64)>, b: (usize, f64)| {
            a.is_some_and(|a| a.0 == b.0 && (a.1 - b.1).abs() < 1e-3)
        };
        assert!(
            near(drawn_at(&s, None), (0, 3.0)),
            "{:?}",
            drawn_at(&s, None)
        );
        assert!(near(drawn_at(&s, Some(0.0)), (0, 3.5)));
        let ahead = drawn_at(&s, Some(2.0 / 30.0));
        assert!(near(ahead, (0, 5.5)), "{ahead:?}");
        assert_eq!(
            drawn_at(&dereth_animation::seq::Sequence::new(), None),
            None
        );
    }

    /// A run whose animation steps the object on a tenth of a meter a frame, played at `rate`
    /// frames a second, with the motion's own steady speed `steady`.
    fn stepping_run(rate: f32, steady: Vec3) -> dereth_animation::seq::Sequence {
        use dereth_animation::data::{AnimData, AnimFrame, AnimationData, MapAssets};
        let id = DataId(0x0300_0002);
        let mut assets = MapAssets::default();
        assets.animations.insert(
            id.0,
            std::sync::Arc::new(AnimationData {
                num_frames: 10,
                num_parts: 1,
                pos_frames: Some(vec![
                    Frame::new(
                        Vec3::new(0.0, 0.1, 0.0),
                        dereth_primitives::Quat::IDENTITY
                    );
                    10
                ]),
                part_frames: (0..10)
                    .map(|_| AnimFrame {
                        frames: vec![Frame::default()],
                        hooks: Vec::new(),
                    })
                    .collect(),
                has_hooks: false,
            }),
        );
        let mut s = dereth_animation::seq::Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: -1,
                framerate: rate,
            },
            &assets,
        );
        s.set_velocity(steady);
        s
    }

    /// A body's cycle asks the speed its animation steps its object on at the rate it plays, with
    /// the motion's own steady speed, whether or not the object moves: a run of a tenth of a meter
    /// a frame at thirty frames a second asks three meters a second ahead, and played backwards,
    /// three behind.
    #[test]
    fn a_cycle_asks_the_speed_its_animation_steps_at_and_the_motions_own() {
        let near = |a: glam::Vec3, b: glam::Vec3| (a - b).length() < 1e-4;
        let ahead = cycle_velocity(&stepping_run(30.0, Vec3::new(0.0, 0.0, 0.0)));
        assert!(near(ahead, glam::Vec3::new(0.0, 3.0, 0.0)), "{ahead:?}");
        let behind = cycle_velocity(&stepping_run(-30.0, Vec3::new(0.0, 0.0, 0.0)));
        assert!(near(behind, glam::Vec3::new(0.0, -3.0, 0.0)), "{behind:?}");
        let both = cycle_velocity(&stepping_run(15.0, Vec3::new(0.5, 0.0, 0.0)));
        assert!(near(both, glam::Vec3::new(0.5, 1.5, 0.0)), "{both:?}");
        assert!(near(
            cycle_velocity(&dereth_animation::seq::Sequence::new()),
            glam::Vec3::ZERO
        ));
    }

    /// A run's stopping link that steps its object on five centimeters a frame at thirty frames a
    /// second for its first five frames, then stands for five; then the stand it stops in.
    fn stopping_link() -> dereth_animation::seq::Sequence {
        use dereth_animation::data::{AnimData, AnimFrame, AnimationData, MapAssets};
        let mut assets = MapAssets::default();
        for (id, step) in [(0x0300_0003, Some(0.05)), (0x0300_0004, None)] {
            assets.animations.insert(
                id,
                std::sync::Arc::new(AnimationData {
                    num_frames: 10,
                    num_parts: 1,
                    // Five frames of steps, then five standing.
                    pos_frames: step.map(|y| {
                        (0..10)
                            .map(|k| {
                                let y = if k < 5 { y } else { 0.0 };
                                Frame::new(
                                    Vec3::new(0.0, y, 0.0),
                                    dereth_primitives::Quat::IDENTITY,
                                )
                            })
                            .collect()
                    }),
                    part_frames: (0..10)
                        .map(|_| AnimFrame {
                            frames: vec![Frame::default()],
                            hooks: Vec::new(),
                        })
                        .collect(),
                    has_hooks: false,
                }),
            );
        }
        let mut s = dereth_animation::seq::Sequence::new();
        for id in [0x0300_0003, 0x0300_0004] {
            s.append_animation(
                AnimData {
                    anim_id: DataId(id),
                    low_frame: 0,
                    high_frame: -1,
                    framerate: 30.0,
                },
                &assets,
            );
        }
        s
    }

    /// A link asks the speed its own step carries the object at on the frame it plays, while the
    /// stand it leads to asks none: a run's stopping stride of five centimeters a frame at thirty
    /// frames a second carries it a meter and a half a second, then nothing once it stands; over
    /// the whole link, half that.
    #[test]
    fn a_stopping_link_asks_the_speed_its_step_carries_the_object() {
        let s = stopping_link();
        let near = |a: glam::Vec3, b: glam::Vec3| (a - b).length() < 1e-4;
        let step = node_velocity(&s, 0, Some(2.5));
        assert!(near(step, glam::Vec3::new(0.0, 1.5, 0.0)), "{step:?}");
        let stands = node_velocity(&s, 0, Some(7.0));
        assert!(near(stands, glam::Vec3::ZERO), "{stands:?}");
        let whole = node_velocity(&s, 0, None);
        assert!(near(whole, glam::Vec3::new(0.0, 0.75, 0.0)), "{whole:?}");
        assert!(near(cycle_velocity(&s), glam::Vec3::ZERO));
        assert!(near(node_velocity(&s, 1, Some(3.0)), glam::Vec3::ZERO));
    }

    #[test]
    fn the_near_and_far_planes_come_back_out_of_the_projection() {
        let p = glam::Mat4::perspective_lh(1.0, 1.5, 0.25, 900.0);
        let (near, far) = planes(p);
        assert!((near - 0.25).abs() < 1e-4, "{near}");
        assert!((far - 900.0).abs() < 0.5, "{far}");
    }

    /// What a block's cover grows from: nothing but `lifts`.
    fn inputs(lifts: u64) -> CoverInputs {
        CoverInputs {
            vertices: Vec::new(),
            cuts: Vec::new(),
            water: Vec::new(),
            words: [0; 81],
            buildings: Vec::new(),
            covers: Vec::new(),
            lifts,
            knobs: dereth_terrain::land::drawn::Knobs::default(),
        }
    }

    /// A row of ten placements, each numbered by its row and place.
    fn row(_: &CoverInputs, row: usize, out: &mut Vec<rm::Placement>) {
        for k in 0..10u32 {
            out.push(rm::Placement {
                // LINT-OK: a test's few rows.
                #[allow(clippy::cast_possible_truncation)]
                id: (row as u32) << 8 | k,
                kind: rm::PlacementKind::Object,
                frame: glam::Mat4::IDENTITY,
                scale: 1.0,
                lifts: [0.0; 2],
                parts: Vec::new(),
            });
        }
    }

    /// **A block's ground cover grows a share a frame and is sent whole, on a frame that grows
    /// none**: within an allowance it is grown a row at a time over frames, never all on one;
    /// the frame it grows whole it waits, and the next frame grows no cover, its own or another
    /// block's, and sends it; what is sent is what growing it at once makes, in the same order.
    #[test]
    fn a_blocks_cover_grows_a_share_a_frame_and_is_sent_whole_on_a_frame_that_grows_none() {
        let mut cache = HashMap::new();
        let mut frames = 0;
        let mut rows_grown = Vec::new();
        let mut whole_last = false;
        let grown = loop {
            frames += 1;
            let mut budget = if whole_last {
                CoverBudget::after(true)
            } else {
                CoverBudget {
                    left: 25,
                    whole: false,
                }
            };
            let before = rows_grown.len();
            let mut counted = |i: &CoverInputs, r: usize, out: &mut Vec<rm::Placement>| {
                rows_grown.push(r);
                row(i, r, out);
            };
            let sent = Cover::grow(
                cache.entry(1),
                inputs(7),
                8,
                Some(&mut budget),
                &mut counted,
            )
            .map(<[_]>::to_vec);
            // Another block, its cover not grown yet, on the same frame.
            let other = Cover::grow(
                cache.entry(2),
                inputs(7),
                8,
                Some(&mut budget),
                &mut counted,
            );
            if let Some(g) = sent {
                assert_eq!(
                    rows_grown.len(),
                    before,
                    "the frame that sends it grows none"
                );
                assert!(other.is_none());
                break g;
            }
            whole_last = budget.whole;
            assert!(frames < 8, "it grows on");
        };
        assert_eq!(frames, 4, "three rows a frame, of eight, then sent");
        assert_eq!(
            rows_grown[..8],
            (0..8).collect::<Vec<_>>(),
            "each row once, in order"
        );
        let mut whole = HashMap::new();
        let at_once = Cover::grow(whole.entry(1), inputs(7), 8, None, row).map(<[_]>::to_vec);
        assert_eq!(Some(grown), at_once);
    }

    /// **A block built again grows its cover again only from something new**: the same inputs
    /// take the cover grown, and new ones grow it again from its first row.
    #[test]
    fn a_blocks_cover_is_grown_again_only_from_something_new() {
        let mut cache = HashMap::new();
        let asked = std::cell::Cell::new(0);
        let counted = |i: &CoverInputs, r: usize, out: &mut Vec<rm::Placement>| {
            asked.set(asked.get() + 1);
            row(i, r, out);
        };
        assert!(Cover::grow(cache.entry(1), inputs(7), 4, None, counted).is_some());
        assert!(Cover::grow(cache.entry(1), inputs(7), 4, None, counted).is_some());
        assert_eq!(asked.get(), 4, "grown once");
        let mut none = CoverBudget::after(true);
        assert!(Cover::grow(cache.entry(1), inputs(8), 4, Some(&mut none), counted).is_none());
        assert!(Cover::grow(cache.entry(1), inputs(8), 4, None, counted).is_some());
        assert_eq!(asked.get(), 8, "grown again, whole, for new lifts");
    }

    #[test]
    fn an_animated_statics_key_names_the_block_its_cell_is_in() {
        assert_eq!(
            animated(0xA9B4_0021, 7),
            rm::ObjectKey::Animated {
                block: 0xA9B4,
                index: 7
            }
        );
    }
}
