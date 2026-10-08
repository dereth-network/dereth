//! Shared scene records and public scene views.

#[path = "appearance.rs"]
mod appearance;
pub use appearance::*;
#[path = "region.rs"]
mod region;
pub use region::*;
#[path = "land.rs"]
mod land;
#[path = "lighting.rs"]
mod lighting;
pub(crate) use lighting::*;
#[path = "baking.rs"]
mod baking;
use baking::*;
#[path = "parts.rs"]
mod parts;
pub(crate) use parts::*;
#[path = "degrade.rs"]
mod degrade;
#[path = "emitters.rs"]
mod emitters;
#[path = "probes.rs"]
mod probes;
#[path = "streaming.rs"]
mod streaming;
use degrade::*;
#[path = "views.rs"]
mod views;
pub use views::*;
#[path = "cells.rs"]
mod cells;
#[path = "objects.rs"]
mod objects;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use dereth_assets::world::{CellLandblock, LandblockInfo};
use dereth_assets::Decode;
use dereth_client_runtime::world_stream::{LandblockWindow, SlotAction, SlotMesh};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{CellId, DataId, Frame, Quat, TextureHandle, Vec3};
use dereth_render::device::{
    Gpu, MergeSource, PerDrawConstants, PerFrameConstants, TerrainMergeJob, TerrainMergeOverlay,
    TerrainSplat, TerrainSplatOverlay, TextureSlot,
};
use dereth_render::palette::ExpandedPalette;
use dereth_render::pso::{PipelineKey, SurfaceContext};
use dereth_render::surface::{Surface as RenderState, SurfaceHandler};
use dereth_render::vertex::VertexFormat;
use dereth_render::{Cull, DrawConstants, RenderError, ViewParams, ZFunc};
use dereth_terrain::consts::BLOCK_LENGTH;
use dereth_world_render::land::emit::{
    detail_vertex, triangle_vertices, visible_triangles, LAND_VERTEX_STRIDE,
};
use {
    dereth_terrain::land::lighting::bake_lighting,
    dereth_terrain::land::lighting::LandscapeLighting,
};
use {
    dereth_terrain::land::merge::land_texture_scale_shift, dereth_terrain::land::merge::Bgra8,
    dereth_terrain::land::merge::MergeKey, dereth_terrain::land::merge::MergePlan,
    dereth_terrain::land::merge::TerrainMergeCache,
    dereth_terrain::land::merge::TerrainTextureSource,
};
use {
    dereth_terrain::land::mesh::generate_landblock_with_table,
    dereth_terrain::land::mesh::height_table, dereth_terrain::land::mesh::LandblockMesh,
};
use {dereth_terrain::land::order::block_draw_order, dereth_terrain::land::order::cell_draw_order};
// The light pool, the eight slots and the D3DLIGHT9s, from the one transcription.
use dereth_terrain::math::V3;
use dereth_terrain::scenery::outside_cell_index;
use dereth_world_render::lighting::{
    ambient_render_state, calc_object_light, enabled_lights, minimize_envcell_lighting,
    minimize_object_lighting, set_color32, sunlight_light, use_sunlight_set, viewer_light,
    world_ambient, ActiveLights, D3dLight, LightInfo, LightPools, LightType, BYTE_TO_FLOAT,
    INDOOR_AMBIENT_LEVEL,
};

use dereth_animation::parts::MaterialOverride;
use dereth_animation::MotionDriver;
// The state word and `set_state`'s three reactions, from the one
// transcription of them in the workspace.
use dereth_primitives::{ObjectId, Position, ServerTime};

use crate::particles::{
    EmitterHost, EmitterPlacement, ParticleGeometry, ParticleGfx, ParticlePart, ParticleStats,
    StaticSlot,
};
use crate::textures::TextureStore;
use dereth_client_runtime::audio::SoundTrigger;
use dereth_client_runtime::character::RenderSpace;
use dereth_client_runtime::models::{build_gfxobj, resolve_parts};
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::world_build::{
    self, interior_content, land_content, read_landblock, BlockStatics, LandContent,
};
use dereth_client_runtime::world_objects;
use dereth_client_runtime::world_state::WorldState;
use dereth_client_runtime::world_step;
use dereth_world_data::anim_assets::DatAnimAssets;

#[cfg(test)]
use dereth_client_runtime::camera::FreeCamera;
use dereth_client_runtime::environment::EnvironmentOverrideState;
use dereth_client_runtime::frame_events::RenderPrefWork;
use dereth_client_runtime::render_prefs::{RegionStyle, RequiredFiles};
use dereth_client_runtime::scene::SceneConfig;
#[cfg(test)]
use dereth_client_runtime::world_build::read_lbi;
use {
    dereth_world_data::landblock::block_xy, dereth_world_data::landblock::load_region,
    dereth_world_data::landblock::WorldError, dereth_world_data::landblock::DERETH_REGION,
};
#[cfg(test)]
use {dereth_world_data::landblock::landblock_did, dereth_world_data::landblock::lbi_did};

/// Which of the cell renderer's two object passes a call to
/// `WorldScene::draw_object_pass` is.
///
/// Cell drawing draws the objects of the cells it reaches in two places, on either side of the
/// depth clear: the outdoor pass draws the outdoor blocks *and their
/// landcells' objects* through the openings, and the per-cell object draw draws each interior
/// cell's objects at the end. [`Self::All`] is the un-split pass every frame that never
/// reaches `IndoorStep::OutdoorsThroughPortals` takes — an outdoor viewer, and an interior
/// with no opening in sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectPhase {
    /// Outdoor landcell objects and the building environment cells the traversal reached.
    Outdoors,
    /// The interior pass: everything else, held objects included.
    Interior,
    /// Both, in one sorted pass.
    All,
}

/// What the last [`WorldScene::draw`]'s per-object frustum test did.
///
/// Four numbers rather than one: a cull that is not running and a cull that is running and
/// rejecting nothing produce the same picture, and `outside == 0` alone cannot tell them
/// apart. `tested` is the denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectConeStats {
    /// Parts offered to the view-cone test because they had a drawing sphere.
    pub tested: u32,
    /// Of those, the ones it answered OUTSIDE for. These are the parts the client
    /// returns `MeshDrawStatus::OutsideViewcone` for without drawing.
    pub outside: u32,
    /// Parts whose selected degrade level has **no** drawing sphere, so the cone could not be
    /// run at all. In retail this state cannot be reached on the draw path: `drawing_sphere`
    /// is a sphere pointer initialized to NULL by both graphics-object constructors and
    /// set only from the drawing BSP, so a mesh without one would make
    /// the cone test dereference NULL. Such a part is **not** culled here, and this counter
    /// exists so that "the cull let it through" is distinguishable from "the cull ran".
    pub no_sphere: u32,
    /// Parts the cull actually skipped the draw for — `outside` when
    /// [`SceneConfig::object_viewcone`] is on, and 0 when it is off.
    pub culled: u32,
}

/// The polygon renderer's clipped and blended deferred lists as
/// one frame of the object pass filled and drained them.
///
/// The alpha-list append adds to the lists and the alpha-list flush drains them; this is
/// the count of both, published because what matters is the list length
/// on the frame rather than the fact that a flush was called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlphaListStats {
    /// Parts submitted this frame -- the denominator. Zero means the scene drew no moving
    /// object at all and every other number here is uninformative.
    pub parts: usize,
    /// Subsets queued on the **clip** list: stippled-or-alpha mask 8, i.e. a
    /// `Base1ClipMap` surface with none of the Alpha bits.
    pub clip: usize,
    /// Subsets queued on the **blend** list: mask 2 (Alpha / InvAlpha / Additive) or mask 4
    /// (Translucent).
    pub blend: usize,
    /// Subsets dropped because a list was already at [`dereth_terrain::consts::ALPHA_LIST_CAP`].
    pub dropped: usize,
    /// Subsets the subset draw drew in place -- mask 0, the opaque ones.
    pub immediate: usize,
    /// Entries `FlushAlphaList(0.0)` actually drew. Equal to `clip + blend` on any frame that
    /// flushed; published separately so *"queued"* and *"drawn"* cannot be confused.
    pub flushed: usize,
    /// Of [`Self::flushed`], the entries "Multiple Pass Alpha" queued, which the flush drew
    /// with surface setup's force-alpha argument: the second, blended pass of a clip-mapped
    /// subset. Zero with the option off.
    pub multipass: usize,
}

/// The landscape's two alpha lists, as the last
/// [`WorldScene::draw`] drew them.
///
/// The object pass has [`AlphaListStats`]; this is the same census for the batches the scene
/// bakes out of a landblock's scenery, statics and buildings. `BlockDraw::blended` is keyed on
/// `alpha_blend` alone, so it holds both lists at once, and the one thing that decides the
/// picture is not a count but **which of them is drawn first**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LandscapeAlphaStats {
    /// The **alpha-tested** list: `alpha_blend && alpha_test`, the 1-bit cut-outs, which keep
    /// their depth write (catalogue rows 7-9). Drawn in the landscape pass.
    pub clip: usize,
    /// The **blended** list drained **early**, by a building's own alpha flush.
    /// That flush is the client's own, so its draws are not counted against the order below.
    pub blend_early: usize,
    /// The **blended** list drained by the frame's own alpha flush -- after the object pass on
    /// an outdoor frame, or before the depth clear on an indoor one.
    pub blend: usize,
    /// Of [`Self::blend`], how many were drawn **before the last** alpha-tested batch. Zero is
    /// the client's order -- the alpha-tested list to exhaustion, then the blended one -- and
    /// anything else means a blended batch, which writes no depth, was laid down early enough
    /// for a later alpha-tested one to paint over it: distant tree foliage covering a near
    /// plant.
    pub blend_before_clip: usize,
    /// The second, blended passes "Multiple Pass Alpha" gave alpha-tested batches at the
    /// frame's alpha flush, drawn before that flush's blended batches. Zero with the option
    /// off.
    pub multipass: usize,
}

/// One draw out of the alpha lists, by kind, as [`WorldScene::drawn_alpha_order`] records
/// them in device order.
///
/// The client keeps one clip list and one alpha list and drains the whole clip list before
/// any of the alpha list at every flush, so within one flush (from one [`Self::FlushStart`] to
/// the next) no clip-list kind follows a blend-list kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaDraw {
    /// A flush begins: a building's own, or the object pass's, which the frame's blended
    /// landscape batches then continue.
    FlushStart,
    /// Clip list: an animated part's alpha-tested entry.
    PartClip,
    /// Clip list: an animated part's "Multiple Pass Alpha" second pass.
    PartForced,
    /// Clip list: a landscape batch's "Multiple Pass Alpha" second pass.
    StaticForced,
    /// Clip list: a particle's alpha-tested entry.
    ParticleClip,
    /// Clip list: a particle's "Multiple Pass Alpha" second pass.
    ParticleForced,
    /// Alpha list: an animated part's blended entry.
    PartBlend,
    /// Alpha list: a particle's blended entry.
    ParticleBlend,
    /// Alpha list: a landscape batch's blended draw.
    StaticBlend,
}

impl AlphaDraw {
    /// Whether this draw belongs to the clip list.
    #[must_use]
    pub const fn is_clip_list(self) -> bool {
        matches!(
            self,
            Self::PartClip
                | Self::PartForced
                | Self::StaticForced
                | Self::ParticleClip
                | Self::ParticleForced
        )
    }
}

/// Which of the two points the client issues its alpha flush from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaFlush {
    /// The first thing a building draws, before its portal pass and its shell.
    Building,
    /// The frame's own, issued by the world pass once the landscape walk and the objects that
    /// walk drew are both on the screen -- or, on an indoor frame that sees outdoors through
    /// an opening, between that outdoor pass and the depth clear.
    Frame,
}

/// One subset of one part, as [`WorldScene::draw`] submitted it.
///
/// Three questions in one row of the trace: *in what order* was it
/// submitted (the part sort), *which list* did it go on
/// (the deferred-list insertion), and *which surface* decided that -- published as
/// a dat id so a test can re-read the surface record and re-derive the answer instead of asking
/// the scene to agree with itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartSubsetDraw {
    /// The server object this part belongs to, or `None` for the **local body**.
    pub object: Option<ObjectId>,
    /// Its index in the object's part array.
    pub part: usize,
    /// Which subset of `gfxobj[deg_level]` this is -- the alpha-list append's surface number.
    pub subset: usize,
    /// The part's viewer distance, the sort key. The sequence of these over the whole trace is
    /// non-increasing exactly when the sort ran.
    pub cypt: f32,
    /// The surface record the subset was collected under, or `None` when the group named none.
    pub surface: Option<DataId>,
    /// The mesh's stippled-or-alpha mask for this subset.
    pub mask: u32,
    /// Which deferred list it went on, or `None` when the subset draw drew it in place.
    pub list: Option<dereth_world_render::objects::alpha::AlphaList>,
    /// The alpha-list append's first-of-kind flag -- the first deferred subset of this part **on
    /// this list**, which is the entry that records the object-to-world matrix and the
    /// material.
    pub first_of_kind: bool,
    /// Whether the object's cell put this subset in the landscape's outdoor half. This is
    /// also the sunlight/static-light-set choice carried by [`PartSubmission::outdoors`].
    pub outdoors: bool,
    /// Whether this subset reached the device before cleared depth.
    ///
    /// This is deliberately independent of [`Self::outdoors`]: an interior object reached
    /// through an outdoor building portal is pre-clear but still uses env-cell lighting.
    pub before_depth_clear: bool,
    /// Whether this draw went down with surface setup's force-alpha argument: blended, not
    /// alpha-tested, no depth write. Set only on the alpha-list flush's draw of an entry
    /// "Multiple Pass Alpha" queued.
    pub force_alpha: bool,
}

/// The frame-rate estimate and adaptive-degrade governor shared by the whole client.
///
/// Both are globals in the client and both are read by more than
/// one subsystem, which is why they are a struct on the scene rather than a field of whatever
/// happens to consume them first.
#[derive(Debug, Clone)]
pub struct DegradeState {
    /// Frame-rate estimate over a 20-frame window.
    pub frame_rate: dereth_client_runtime::camera::FrameRate,
    /// Degrade multiplier and the 30-entry history that damps it.
    pub governor: dereth_world_render::degrade_loop::DegradeGovernor,
    /// How many times the governor has run, for the log line and the tests.
    pub frames: u64,
}

impl DegradeState {
    /// A pinned governor — [`dereth_terrain::consts::PINNED_DEG_MUL`] — unless the scene
    /// asked for the live loop.
    #[must_use]
    pub fn new(auto: bool) -> Self {
        use dereth_world_render::degrade_loop::{DegradeGovernor, FramerateTargets};
        let governor = if auto {
            DegradeGovernor::automatic(FramerateTargets::default())
        } else {
            DegradeGovernor::pinned(dereth_terrain::consts::PINNED_DEG_MUL)
        };
        Self {
            frame_rate: dereth_client_runtime::camera::FrameRate::default(),
            governor,
            frames: 0,
        }
    }
}

/// One window slot that has geometry.
///
/// Everything in it is **block-local**: landscape frame calculation keeps the terrain
/// vertices in 0..192, and the object batches are too. Block drawing opens by
/// pushing the block's frame (block id, identity) at position level 3, so every draw inside a block — terrain,
/// building, object — is issued in that block's frame. Baking to it is what makes a window
/// scroll cost one matrix per block instead of re-transforming 53,526 triangles.
#[derive(Debug)]
struct BlockDraw {
    /// The landscape block origin, viewer-block-relative. Recomputed on
    /// every scroll; nothing else about the block changes.
    origin: (f32, f32),
    /// `CellLandblock::terrain`, one 16-bit word per vertex, indexed `row * 9 + col`. Terrain
    /// lighting reads exactly this and nothing else —
    /// no day/night term, no weather, no season — so the block has to keep it.
    terrain: [u16; dereth_assets::world::VERTEX_COUNT],
    /// The landscape surface-cache entry for each cell, indexed `i * side_cell_count + j`. Rebuilt whenever
    /// the slot changes LOD ring, because the cell count changes with it.
    cell_texture: Vec<Option<TextureSlot>>,
    /// The merge key each of those cells took a reference on, in the same order.
    ///
    /// This is polygon zero's positive-side surface field, which block teardown
    /// reads from every cell in order to release the merged surface once per cell. The
    /// block has to carry it because the block is what leaves the window, and the
    /// `LandblockMesh` that produced the keys lives in `LandblockWindow` and is replaced
    /// before the departing block is torn down.
    ///
    /// Kept beside `cell_texture` rather than folded into it because a cell whose merged
    /// texture failed to upload still took a reference — `get_or_build` increments
    /// the surface's cell count on every call — so a `None` texture is not a cell with no link.
    cell_keys: Vec<MergeKey>,
    /// Every cell's merge key, in draw order, whether or not the block holds composites.
    /// [`Self::cell_keys`] is the same list while it does and empty while it does not, because
    /// that one is the reference book the release paths read.
    merge_keys: Vec<MergeKey>,
    /// Each cell's terrain layers for the splat draw, in the same order, while the landscape
    /// is splatted (or was, and has not been converted back yet). Shared with every other cell
    /// of the same merge key.
    cell_splat: Vec<Option<Arc<TerrainSplat>>>,
    /// Scenery, buildings and statics for this block, in **block-local** space.
    opaque: Vec<StaticBatch>,
    blended: Vec<StaticBatch>,
    /// The parts of those placements that carry a multi-level
    /// `GfxObjDegradeInfo`, and the level each is currently drawing.
    /// `WorldScene::refresh_degrade_levels` re-picks them every frame.
    degrade: Vec<DegradePlacement>,
    /// Whether [`StaticBatch::active`] has ever been assembled for this block. A separate flag
    /// rather than "is `active` empty", because a block every one of whose placements has
    /// degraded past its last real level assembles to *nothing* and would otherwise be
    /// rebuilt on every frame for ever.
    degrade_assembled: bool,
    /// The block's environment cells.
    env_cells: Vec<EnvCellDraw>,
    /// The block's building objects, as the *outdoor* portal machinery needs them.
    /// The shell triangles are in `opaque` like any other static; this is the portal half.
    building_views: Vec<BuildingView>,
    /// Whether this block's objects were baked. A block enters the window at the outermost
    /// ring, which is normally outside `SceneConfig::scenery_radius`, and only later scrolls
    /// into it — so "has objects" is a state of its own and not a synonym for "was fetched".
    baked: bool,
    /// The `LandblockMesh::side_cell_count` those objects were baked at.
    ///
    /// Static-object initialization refuses to build anything at all unless the
    /// count is **8**, and `dereth_world_render::scenery::generate_scenery` carries
    /// that guard as its first line — so a bake taken on an outer LOD ring contains **no
    /// scenery**. A detail-size change destroys the objects when the
    /// count changes, and the next visibility refresh rebuilds them for
    /// any block that is at full detail. Keeping the count is how `WorldScene::build_slot`
    /// knows a `SlotWork::Mesh` has changed the detail level and must not reuse the bake; see
    /// [`SlotWork`].
    baked_side_cell_count: u8,
    /// What the counters in [`SceneStats`] would lose if this block were dropped.
    scenery: usize,
    buildings: usize,
    statics: usize,
    /// The block's **physics** half: its interior cells' statics, its outdoor land cells'
    /// scenery and landblock-info statics, its house restrictions, and whether each has been
    /// registered. The draw half of the statics is baked into `opaque` and the owning
    /// [`EnvCellDraw`]s.
    sim: BlockStatics,
    /// The light objects those statics contribute to their cells' light lists.
    cell_lights: Vec<CellLightObj>,
    /// The statics and scenery of this block whose setup record names a
    /// `default_script`, block-local, found at bake time. This includes the
    /// interior cells' statics, which is where a dungeon's braziers and candles come from.
    emitters: Vec<EmitterPlacement>,
    /// Those placements as live objects. Spawned by `WorldScene::bring_up_particles`
    /// rather than at bake, because a `MotionDriver` needs the `Arc<RetailDatStore>` that
    /// only [`WorldScene::sync_objects`] is handed.
    hosts: Vec<EmitterHost>,
    /// Whether that spawn has run. A separate flag rather than comparing host and emitter
    /// counts, because a placement whose setup record will not load produces no host and
    /// would otherwise be retried — and re-spawning a host restarts its emitters, which on a
    /// permanent one is a flame that never gets past its first particle.
    hosts_spawned: bool,
    /// [`BakedObjects::objects_visual`], kept for the release.
    objects_visual: bool,
}

/// Which block a window slot holds and how `generate` must build it: the landblock's
/// `block_coord`, `dir` and the LOD divisor block orientation assigned.
#[derive(Debug, Clone, Copy)]
struct SlotSpec {
    block_x: i32,
    block_y: i32,
    lod_div: u8,
    dir: dereth_terrain::land::mesh::Direction,
}

/// One block's baked objects and the three counters they contribute to [`SceneStats`].
#[derive(Debug, Default)]
struct BakedObjects {
    opaque: Vec<StaticBatch>,
    blended: Vec<StaticBatch>,
    degrade: Vec<DegradePlacement>,
    env_cells: Vec<EnvCellDraw>,
    building_views: Vec<BuildingView>,
    scenery: usize,
    buildings: usize,
    statics: usize,
    /// The physics half, and whether it already has bodies. Carried across a re-mesh for the
    /// same reason `hosts` is.
    sim: BlockStatics,
    /// The statics' light objects.
    cell_lights: Vec<CellLightObj>,
    /// The placements that carry a setup default script.
    emitters: Vec<EmitterPlacement>,
    /// Those placements as live objects, carried across a re-mesh: a detail-size change
    /// rebuilds the terrain arrays and leaves static objects alone, so an LOD
    /// change must not restart a torch's flame.
    hosts: Vec<EmitterHost>,
    hosts_spawned: bool,
    /// Whether a texture this bake wanted was left pending on the mip worker, so the bake has
    /// to be handed back and taken again. See [`crate::mip_worker`].
    textures_pending: bool,
    /// Whether another era's look was set when the block was baked ([`LandContext::objects`]);
    /// each batch and interior cell says whether its own links are that look's.
    objects_visual: bool,
}

/// One resident block's bake as [`WorldScene::block_bake`] reports it.
///
/// The three populations are **separate fields on purpose**, so that a total over them can
/// never be a differential whose control is another part of itself — which is how
/// `object_triangles > 0` stayed green over a window with no trees left in it.
///
/// `generate_scenery` is not the only population behind the full-detail guard:
/// static-object and building initialization carry the same guard, so on a coarse ring
/// all three populations are 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockBake {
    /// Generated scenery placements.
    pub scenery: usize,
    /// Authored static-object placements.
    pub statics: usize,
    /// Authored building placements.
    pub buildings: usize,
    /// The `LandblockMesh::side_cell_count` the bake was taken at.
    pub side_cell_count: u8,
    /// Whether the bake ran at all, i.e. whether the slot is inside
    /// [`SceneConfig::scenery_radius`]. The denominator for every zero above.
    pub baked: bool,
}

/// One window slot as [`WorldScene::window_rings`] reports it, for the streaming test: what it
/// holds, the detail it was meshed at, and the detail its ring asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRing {
    pub xi: i32,
    pub yi: i32,
    pub block: (i32, i32),
    pub side_cell_count: u8,
    pub expected_side_cell_count: u8,
}

/// One interior cell of a block, ready to draw.
///
/// Cell drawing submits the cell's constructed mesh after
/// installing the cell's surfaces and pushing the cell's own frame. The frame is
/// folded into the vertices here for the same reason the block frame is folded into everything
/// else: a cell never moves inside its block, so the only matrix left is the
/// block's.
#[derive(Debug)]
struct EnvCellDraw {
    id: CellId,
    /// The cell's placement frame inside the landblock. Kept because the
    /// portal traversal works in **cell** space even though the mesh does not.
    frame: Frame,
    portals: Vec<EnvPortal>,
    /// The cell's geometry, in block-local space.
    meshes: Vec<PartMesh>,
    /// Whether [`Self::meshes`] are the other era's record of the room, whose surface cache
    /// holds their links ([`ObjectLook`]). Each static batch says so for itself.
    from_look: bool,
    /// The cell's static-object triangles, block-local, split the way
    /// the polygon renderer splits them: what draws immediately, and what is deferred.
    /// `IndoorStep::Objects` draws the first and `IndoorStep::FlushAlphaList`
    /// the second.
    statics: Vec<StaticBatch>,
    statics_blended: Vec<StaticBatch>,
    /// This cell's degrading static placements. Per cell rather than per block,
    /// because the batches are.
    degrade: Vec<DegradePlacement>,
    /// The parts of those statics one by one, and the other cells each is registered in.
    objects: Box<CellObjects>,
    /// The mesh's burned-static-lights marker: `None` until
    /// the static-light burn has written the static light into the
    /// meshes' vertex colours, then the static-light count it was burned at -- the cache key
    /// the client uses: bit 7 set and `& 0x7F ==` the static-light count means
    /// "already current"), so a pool that swaps one light for another at the same count does
    /// not re-burn, exactly as retail does not.
    burned_count: Option<usize>,
}

/// A cell's baked statics as cell drawing offers them: part by part.
///
/// The client keeps an object's parts, not its triangles, in every cell the object is
/// registered in. Each cell's object draw tests every part it holds against that cell's view
/// polygons with the part's drawing sphere and draws the ones that pass, each once a frame
/// whichever cell draws it first. A static is registered in its own cell and in every cell its
/// geometry reaches: a neighbouring room through an inner doorway, or the land cells outside
/// through an outdoor one. So a part can be drawn by a cell other than its own, including by a
/// land cell from outdoors whatever the building's shell is drawing, and a part of a reached
/// cell can be left out because no view of that cell's sees it.
///
/// The batches stay merged by surface; [`StaticBatch::part_runs`] says which bytes are which
/// part's, so a frame that leaves a part out draws the rest of the batch without it.
#[derive(Debug, Default)]
struct CellObjects {
    /// Every baked part of the cell's statics, in bake order; [`PartRun::part`] indexes it.
    parts: Vec<CellStaticPart>,
    /// The parts of **other** cells' statics registered in this cell, as `(owning cell id,
    /// part index there)`.
    guests: Vec<(u32, u32)>,
    /// This cell's parts registered in land cells, as `(land cell id, part index)`.
    outdoors: Vec<(u32, u32)>,
}

/// One part of a baked cell static, as the per-object test and the once-a-frame rule need it.
#[derive(Debug)]
struct CellStaticPart {
    /// The static's index among the cell's statics, in the dat's order.
    object: u32,
    /// The static's setup (or graphics-object) id.
    setup: DataId,
    /// The degrade placement among the cell's [`EnvCellDraw::degrade`] whose level is the
    /// part's, or `None` for a part with no degrade record (always level 0).
    placement: Option<u32>,
    /// The part's block-local frame as baked: its draw position unless it billboards.
    frame: Frame,
    /// The object scale the cone test scales the sphere by, the largest axis of the part's
    /// graphics-object scale.
    scale: f32,
    /// Each level's drawing sphere, part-local; one entry for a part with no record. `None` is a
    /// level that draws nothing; a mesh without a sphere has [`NO_DRAWING_SPHERE`]. Shared by
    /// every part of the same graphics object.
    spheres: LevelSpheres,
    /// `(frame stamp, cell id, turn)` of the last draw: which frame drew the part, which cell's
    /// object draw did, and which of that draw's turns took it ([`SceneDraw::frame_turn`]).
    /// A part is drawn once a frame, by the one turn that took it; the stamp moves on between
    /// an indoor frame's outdoor pass and its interior one, so the second may draw it again.
    drawn: std::cell::Cell<(u32, u32, u32)>,
}

/// Which bytes of a [`StaticBatch`]'s drawn buffer belong to which part of its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PartRun {
    /// The index in the owning cell's [`CellObjects::parts`].
    part: u32,
    start: u32,
    end: u32,
}

/// [`LevelChunk::part`] for geometry no cell part owns: a landblock's batches.
const NO_PART: u32 = u32::MAX;

/// Each level's drawing sphere of a graphics object; see [`CellStaticPart::spheres`].
type LevelSpheres = Arc<[Option<(Vec3, f32)>]>;

/// The sphere of a cell part's mesh that has no drawing sphere: big enough that no view
/// rejects it, so the part is drawn as it was before the test, the way the object draw lets
/// such a part through.
const NO_DRAWING_SPHERE: (Vec3, f32) = (Vec3::new(0.0, 0.0, 0.0), f32::MAX);

/// One part of a resident interior cell's baked statics, as the last draw left it. See
/// [`WorldScene::cell_static_parts`].
#[derive(Debug, Clone, PartialEq)]
pub struct CellStaticPartProbe {
    /// The cell the static was placed in.
    pub cell: CellId,
    /// The static's index among the cell's statics.
    pub object: u32,
    /// The part's index among the cell's baked parts.
    pub part: u32,
    /// The static's setup (or graphics-object) id.
    pub setup: DataId,
    /// The other cells the static is registered in, interior and land.
    pub registered: Vec<CellId>,
    /// The part's drawing sphere at its level this frame, in the renderer's space, or `None`
    /// for a level that draws nothing.
    pub sphere: Option<(Vec3, f32)>,
    /// The cell whose object draw drew the part on the last draw, or `None` if no cell did.
    pub drawn_by: Option<CellId>,
}

/// What the per-part cell-object draw did on one frame. See [`WorldScene::drawn_cell_statics`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellStaticDrawStats {
    /// Parts offered to a cell's object draw that had not been drawn yet this frame.
    pub offered: u32,
    /// Of those, the ones every view of the offering cell rejected.
    pub outside: u32,
    /// The ones left out for it: `outside` with [`SceneConfig::object_viewcone`] on, 0 off.
    pub culled: u32,
    /// Parts drawn by their own cell.
    pub drawn_home: u32,
    /// Parts drawn by another interior cell they are registered in.
    pub drawn_guest: u32,
    /// Parts drawn by a land cell they are registered in.
    pub drawn_outdoors: u32,
    /// Batches submitted whole, with every one of their parts drawn by the submitting cell.
    pub batches_whole: u32,
    /// Batches submitted with some of their parts left out.
    pub batches_filtered: u32,
    /// Turns given to a building's room already reached through another of its openings this
    /// frame: each such turn offers again only what the earlier turns left out.
    pub cells_reached_again: u32,
}

/// Which submission of a cell's static batch a [`CellStaticRunDraw`] was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CellRunPass {
    /// Drawn where the cell's object draw reached it.
    Direct,
    /// The "Multiple Pass Alpha" second draw of a clip-mapped batch.
    MultiPass,
    /// Queued on the alpha list and drawn when it was flushed.
    AlphaList,
}

/// One part's run of a cell's static batch as the last draw submitted it. A run is one
/// part's bytes in one batch; a part with several surfaces has a run in each of its batches.
/// See [`WorldScene::drawn_cell_static_runs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellStaticRunDraw {
    /// Which of the frame's passes submitted it: 0 for the first, 1 for an indoor frame's
    /// interior pass after its outdoor one.
    pub pass_of_frame: u32,
    /// The cell whose static the part is.
    pub cell: CellId,
    /// Whether the batch is in the cell's blended list rather than its opaque one.
    pub blended_list: bool,
    /// The batch's index in that list.
    pub batch: u32,
    /// The part's index among the cell's baked parts.
    pub part: u32,
    /// Which submission of the batch it was.
    pub pass: CellRunPass,
    /// The cell whose object draw drew it.
    pub by: CellId,
}

/// One light-object entry in a resident interior cell's light list -- what
/// light initialization makes for each setup-light entry of an object
/// with `LIGHTING_ON_PS` (the constructor's `0x400C08` has it), and light registration uses the
/// cell the object entered. For a cell static
/// (made with no parent and no flags, so `STATIC_PS` is
/// set), light initialization writes `state |= 1`, making it a **static** light.
#[derive(Debug, Clone, Copy)]
struct CellLightObj {
    /// The cell the object was added to.
    cell: CellId,
    /// The light-object entry's setup light info, offset relative to the object.
    info: LightInfo,
    /// The light object's global offset -- the object's frame, **block-local** like every other
    /// frame a block holds.
    frame: Frame,
    /// Bit 0 of the light object's state.
    is_static: bool,
}

/// One cell portal as the traversal needs it: the connection, plus the polygon in cell space.
#[derive(Debug)]
struct EnvPortal {
    portal_side: u8,
    other_cell_id: u32,
    other_portal_id: i32,
    exact_match: bool,
    /// The portal polygon's vertices, in the owning cell's space.
    vertices: Vec<Vec3>,
    /// Its plane, also in cell space.
    plane_normal: Vec3,
    plane_d: f32,
}

/// One building object as the **outdoor** portal machinery needs it.
///
/// Outdoor portal traversal needs three things a baked triangle batch throws
/// away: the building's outdoor portal list, its shell graphics object's **drawing** BSP,
/// and the portal polygons named by that BSP's portal nodes. The
/// shell's triangles stay in [`BlockDraw::opaque`] where the bake put them.
///
/// **The openings belong to the level the shell draws.** Both halves of a building's draw,
/// the openings pass and the shell, take the graphics object of the shell's current degrade
/// level, so the openings are kept per level ([`Self::levels`]) and the pass walks the level
/// [`Self::shell_placement`] selected this frame. A level with no openings opens nothing: no
/// stamp, no cell, nothing inside is drawn from outdoors.
#[derive(Debug)]
struct BuildingView {
    /// One-based outdoor land-cell index. Native reaches this building through that cell's
    /// sort-cell draw, after its terrain and before the nearer cells in the draw-order walk.
    cell_index: u16,
    /// `BuildInfo::frame` — the building's placement inside the block.
    /// Portal traversal runs inside this position push, so the BSP planes and polygons below are in
    /// **building** space and the viewpoint is transformed into it.
    frame: Frame,
    /// The openings of each of the shell's degrade levels, nearest level first. `None` is a
    /// level that opens nothing: its graphics object id is 0, or its drawing BSP has no
    /// portal node. A shell with no degrade record, or baked with levels off, has the one
    /// level-0 entry. Levels that name the same graphics object share one entry.
    levels: Vec<Option<Arc<LevelOpenings>>>,
    /// The index into the block's degrade placements of the shell part's placement, whose
    /// level is the one the shell draws this frame. `None` for a shell that does not degrade,
    /// which always draws level 0.
    shell_placement: Option<u32>,
    /// The building's outdoor portal list, with `other_cell_id` widened by the landblock base.
    portals: Vec<dereth_world_render::cells::portal_view::BuildingPortal>,
}

/// One shell level's openings: its drawing BSP and the portal polygons its portal nodes name.
#[derive(Debug)]
struct LevelOpenings {
    /// Walked per frame by
    /// `dereth_world_render::cells::portal_view::build_draw_portals_only`, because which portals it
    /// yields, and in what order, depends on where the viewer is.
    bsp: dereth_assets::common::BspTree,
    /// The portals named by the BSP's `in_portals`, keyed by polygon index. Only the
    /// referenced ones are kept — a 226-polygon shell names two.
    // ORDER-OK: a lookup keyed by the polygon index the BSP hands back.
    portal_polygons: BTreeMap<usize, BuildingPortalPolygon>,
}

impl BuildingView {
    fn draw_cell(&self, side_cell_count: u8) -> u16 {
        let native = self.cell_index - 1;
        let n = u16::from(side_cell_count);
        let step = 8 / n.max(1);
        (native / 8 / step) * n + (native % 8 / step)
    }

    /// The degrade level the shell draws this frame, read from its placement among the
    /// block's `degrade` placements; 0 for a shell that does not degrade.
    fn drawn_level(&self, degrade: &[DegradePlacement]) -> usize {
        self.shell_placement
            .and_then(|i| degrade.get(i as usize))
            .map_or(0, |p| p.level as usize)
    }

    /// The openings of the level the shell draws this frame. `None` when that level has none
    /// (or the record has no such level): then nothing inside the building is drawn from
    /// outdoors.
    fn drawn_openings(&self, degrade: &[DegradePlacement]) -> Option<&LevelOpenings> {
        self.levels
            .get(self.drawn_level(degrade))
            .and_then(Option::as_deref)
    }

    /// The full-detail shell's openings: where the building's doors and windows are.
    fn full_openings(&self) -> Option<&LevelOpenings> {
        self.levels.first().and_then(Option::as_deref)
    }
}

/// One portal polygon of a building shell, in the building's own space.
#[derive(Debug)]
struct BuildingPortalPolygon {
    /// The polygon plane, which the sidedness test reads.
    plane_normal: Vec3,
    plane_d: f32,
    /// The polygon's vertices. Kept for the differential test, which projects them to find
    /// where on the screen an interior is allowed to appear.
    vertices: Vec<Vec3>,
}

/// Texture slots a bake took a link on, each with whether the objects' look's cache handed it
/// out ([`ObjectLook`]) rather than the world's.
type TextureLinks = Vec<(TextureSlot, bool)>;

/// Every resident interior cell as the traversal walks it, keyed by full cell id.
///
/// A cell reached through a portal from another block is addressed by its full id, so this spans
/// blocks without needing a separate lookup for each block.
type TraversalCells = BTreeMap<u32, dereth_world_render::cells::portal_view::TraversalCell>;

/// The same cells' draw-side halves: the baked meshes, and the origin of the block they sit in.
type PlacedCells<'a> = BTreeMap<u32, (&'a EnvCellDraw, (f32, f32))>;

/// How much of a window slot [`WorldScene::stream`] has to rebuild.
///
/// The landblock window decides this per slot and [`SlotAction`] is its answer;
/// this is only the coarser question of what the scene has to redo.
///
/// # A block's objects depend on its LOD ring
///
/// It is tempting to assume a block's objects are built once when the block is fetched and
/// survive a detail-size change. They do not, and assuming so makes a re-entered block draw no
/// scenery for the rest of the session.
///
/// Demotion releases visible interior cells, dynamic objects, static objects and buildings,
/// then resets the closest-cell coordinates to `{-1, -1}`. This happens only under
/// `side_cell_count == 8 && 8 / lod_div != 8`: the transition out of full detail.
/// Nothing in that teardown puts the objects back.
///
/// On every viewer-cell change, both indoor and outdoor visibility refreshes walk the
/// **whole** `mid_width * mid_width` window. They initialize buildings and their visibility
/// on every resident block, then initialize static and dynamic objects on every resident
/// block. This is idempotent: object initialization returns unless `side_cell_count == 8`
/// and inside that it **builds only when the block has no static objects** and otherwise
/// merely re-adjusts scene-object heights and re-crosses cells.
///
/// So retail's net behaviour, and the behaviour `WorldScene::build_slot` now reproduces, is:
/// **a change of `side_cell_count` invalidates a block's objects.** Demoted out of full detail
/// they are destroyed and cannot be rebuilt; promoted back to full detail the next
/// visibility refresh finds no static objects and builds them, scenery included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotWork {
    /// `Fetched`: everything, including the objects.
    Full,
    /// `Resized` or `Restitched`: the mesh and its per-cell surfaces. The objects are kept
    /// **only when the detail level did not change** — see the type docs and
    /// `WorldScene::build_slot`'s `reuse`.
    Mesh,
}

/// One pre-baked group of object triangles, already in viewer-block-relative world space.
///
/// The client re-transforms and re-submits every part every frame.
/// A static scene has no animation and no moving objects, so the transform is done once at load;
/// the submission is still one draw per group per frame, because
/// `dereth_render::device::Gpu::draw_dynamic` is the only draw entry point and it re-uploads its
/// vertices each call. A persistent vertex buffer would need a new render-device entry point.
#[derive(Debug)]
struct StaticBatch {
    key: PipelineKey,
    /// [`Self::key`] with `StageOps::SINGLE_PASS_DETAIL` — what
    /// the subset draw asks surface setup for when a detail surface is installed.
    /// Used only by the **building** pass, because
    /// is the only static draw that installs one.
    key_detail: PipelineKey,
    /// [`Self::key`] with surface setup's force-alpha argument set: the second pass "Multiple
    /// Pass Alpha" gives a clip-mapped batch at the alpha flush. See
    /// [`PartMesh::key_force_alpha`].
    key_force_alpha: PipelineKey,
    /// Authored surface-record type, independent of the generated solid-colour texture slot.
    surface_type: u32,
    /// Building drawing's second (shell) pass, not ordinary objects sharing this material.
    building_pass: bool,
    alpha_ref: u8,
    texture: Option<TextureSlot>,
    /// 0 = linear/wrap, 1 = linear/clamp — `Gpu::bind_texture`'s table.
    sampler: u32,
    /// The surface's luminosity, for the subset draw's emissive.
    luminosity: f32,
    /// A bounding sphere of every vertex this batch owns, block-local: the
    /// stand-in for the object-lighting reach test. The client tests each *part's* sphere; a baked batch
    /// merges every static of a cell that shares a surface, so its sphere is the union's --
    /// a superset that can admit a light no single part reaches.
    sphere: (Vec3, f32),
    /// Every vertex this batch owns, in bake order. For a placement whose
    /// `GfxObjDegradeInfo` has more than one level, that is **every level's** mesh, laid
    /// down in level order where a single-level placement's mesh would sit.
    vertices: Vec<u8>,
    /// Which byte range of [`Self::vertices`] belongs to which degrade placement and level.
    ///
    /// **Empty when nothing in this batch degrades**, and then [`Self::vertices`] is drawn
    /// whole — byte for byte the single-level buffer, which is what makes
    /// [`SceneConfig::degrade_levels`] a control rather than a comment.
    chunks: Vec<LevelChunk>,
    /// The subset of [`Self::vertices`] this frame draws, assembled by
    /// `WorldScene::refresh_degrade_levels` when a placement changes level and reused on
    /// every frame in between. `Gpu::draw_dynamic` copies whatever it is handed into the
    /// upload arena every frame regardless, so the cost of holding this is the memory and not
    /// the copy.
    active: Vec<u8>,
    /// Whether this batch was drawn from another era's look, whose surface cache holds its
    /// picture's link ([`LandContext::objects`]); the world's cache holds it otherwise.
    from_look: bool,
    /// Which bytes of the drawn buffer ([`drawn_vertices`]) are which part of the owning
    /// cell's statics, in buffer order. Empty for a landblock's batches, whose objects are
    /// drawn whole.
    part_runs: Vec<PartRun>,
}

/// One run of vertices in a [`StaticBatch`], and the degrade level it belongs to.
///
/// `placement == NO_PLACEMENT` is geometry no `GfxObjDegradeInfo` governs: it is always drawn,
/// and it keeps its position in the buffer so that a batch with no degrading placement in it
/// assembles to exactly the bytes it baked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LevelChunk {
    placement: u32,
    level: u32,
    start: u32,
    end: u32,
    /// The cell part the run belongs to, or [`NO_PART`].
    part: u32,
}

/// [`LevelChunk::placement`] for geometry that never degrades.
const NO_PLACEMENT: u32 = u32::MAX;

/// The data needed to select one baked part's detail level by viewer distance.
///
/// A baked batch cannot hold a part, so this is the part reduced to exactly the three things
/// the selection reads: the point whose camera depth is measured, the vertical scale the
/// distance is divided by, and the degrade record queried. Everything else about the
/// part — its frame, its surfaces, its material — is already in the vertices.
#[derive(Debug, Clone)]
struct DegradePlacement {
    /// `pos.origin + localtoglobalvec(pos.rotation, gfxobj_scale ⊙ gfxobj[0]->sort_center)`,
    /// **block-local**, because the batches are. This is the viewer-distance answer for
    /// a static, whose cell and the viewer's are in the same block frame.
    centre: Vec3,
    /// The owning object's origin, **block-local**: the point the object-level viewer-distance
    /// update measures from when the camera is at or past the object's share distance, and
    /// every part of the object is handed that one distance and heading. `None` for a
    /// building, whose one part always measures itself.
    object_origin: Option<Vec3>,
    /// `gfxobj_scale.z` — `get_degrade` is asked the viewer distance over the z scale, so a scaled-up tree
    /// degrades at proportionally longer range.
    scale_z: f32,
    /// The `GfxObjDegradeInfo` itself. Shared: one `Arc` per record, not per placement, so a
    /// landblock with 900 of the same tree holds one copy of its four bands.
    info: Arc<dereth_assets::motion::GfxObjDegradeInfo>,
    /// The level this placement is currently drawing, i.e. what the last
    /// `WorldScene::refresh_degrade_levels` chose.
    level: u32,
    /// True when any level of [`Self::info`] names a `degrade_mode` the
    /// client's `switch` acts on, i.e. `DegradeMode::from_raw(mode) != None`.
    ///
    /// The predicate is the *decoded* mode and not `mode != 1`, because
    /// `calc_draw_frame`'s `switch` falls through to "do nothing" for every value
    /// it does not name; a record carrying a stray `0` or `7` is not a billboard.
    ///
    /// A placement that billboards has its vertices baked **part-local** (scaled, but not
    /// transformed), and [`assemble_batches`] maps them through [`Self::draw_pos`] every time
    /// it assembles. One that does not keeps the world-space bake untouched, so the bytes
    /// of a non-billboarding batch never change after the bake.
    billboards: bool,
    /// The part's **block-local** world frame, as placed at bake time.
    ///
    /// A static's `pos` never moves — that is what makes it a static — so this is baked once.
    /// It is the *input* to `calc_draw_frame`, which is why it is kept beside the output: with
    /// mode 1 the two are equal and the assembly reproduces the world-space bake exactly.
    frame: Frame,
    /// The part's draw-position frame, i.e.
    /// `calc_draw_frame(frame, mode, viewer_heading)`, as of the last
    /// [`select_levels`]. Equal to [`Self::frame`] whenever the selected mode is `None`.
    draw_pos: Frame,
    /// The billboarding mode of the level being drawn, as
    /// `get_degrade` returned it.
    mode: dereth_world_render::objects::degrade::DegradeMode,
}

/// Defined in `dereth_client_runtime::world_state`, beside the probe that returns it.
pub use dereth_client_runtime::world_state::ObjectPose;

/// Whether this build has a landscape/building **detail-texture pass** — the second texture
/// stage `Render.BuildingDetailTextures` ("Environment Detail Textures" on the options page)
/// switches. The implemented pass sets this to `true`.
///
/// The client rebuilds four detail surfaces and their tiling values when detail texturing
/// changes, then passes the current detail surface to each mesh-subset draw.
///
/// It is a constant rather than a comment so that a test can *assert* the absence: a
/// preference that reaches no subsystem and a preference that is not polled at all print
/// alike otherwise, which is how an unwired preference comes to look wired.
pub const DETAIL_TEXTURE_PASS: bool = true;

/// The whole static scene: terrain, scenery, buildings and the camera that looks at them.
///
/// This is the scene's **drawing half**: the baked landscape, the meshes and
/// textures, the lights and the per-frame draw records. Its methods that read the simulation
/// take the [`WorldState`] as a parameter; [`WorldScene`] is the pair of the two.
#[derive(Debug)]
pub struct SceneDraw {
    pub cfg: SceneConfig,
    /// Shadow copies used by the rendering-preference poll.
    ///
    /// The client keeps one shadow per polled field: landscape and environment texture
    /// detail, landscape draw distance, environment detail textures, projection and gamma.
    /// It compares each against the current preferences on **every frame**; only a field
    /// that differs does any work. [`SceneConfig::render`] holds the current values and this is the
    /// shadow bank, so [`WorldScene::update_from_preferences`] is the same poll.
    render_shadow: dereth_client_runtime::render_prefs::RenderPreferences,
    /// The four generated detail surfaces and their tiling values.
    detail: dereth_world_render::detail::DetailTexturing,
    /// The device texture each generated detail surface wraps, loaded from database type
    /// `0x0B` and installed as its texture map.
    /// Indexed by [`dereth_world_render::detail::DetailClass`].
    detail_textures: [Option<TextureSlot>; 4],
    /// The 3D viewport's rectangle, filled from the world-view panel
    /// element's own screen box. `None` is the whole back buffer.
    ///
    /// Written only by [`WorldScene::set_game_viewport`], whose one production caller is
    /// `App::frame`; read by [`WorldScene::view_params`], which is where it reaches both the
    /// device rect and the projection's aspect.
    game_viewport: Option<dereth_render::camera::Viewport>,
    /// Every resident block's geometry, keyed by `(block_x, block_y)` rather than by window
    /// index so that a scroll moves nothing: [`LandblockWindow::update_block`] already moves
    /// the slots, and this only has to gain what it fetched and lose what it released.
    // ORDER-OK: drained through `block_draw_order`, which is the client's own order; the map's
    // own iteration is used for the counters alone.
    blocks: BTreeMap<(i32, i32), BlockDraw>,
    /// Blocks [`WorldScene::stream`] still has to build. The
    /// re-centre is cheap and happens inside the frame's simulation step; the fetch needs the
    /// device and happens outside the frame bracket, exactly as a shared-cache fetch does.
    ///
    /// Keyed by `(block_x, block_y)` rather than by window index, because with a
    /// [`SceneConfig::stream_budget`] work can outlive a frame and a scroll in between moves
    /// every slot. The key order is window order, so an unbudgeted stream builds in the order
    /// it always has.
    // ORDER-OK: iterated in key order (window order) or sorted by distance to the viewer.
    pending: BTreeMap<(i32, i32), SlotWork>,
    /// Queued blocks whose last bake was handed back to wait on the mip worker, with the chains
    /// it asked for. [`Self::stream`] skips them until every one is done.
    awaiting: BTreeMap<(i32, i32), Vec<crate::mip_worker::ChainKey>>,
    /// Blocks the scroll pushed out of the window, waiting to give their
    /// bake's texture links back.
    ///
    /// It exists for exactly the reason [`Self::pending`] does, and it is the mirror image:
    /// [`Self::queue`] is the re-centre and is handed no device, while returning a link can
    /// free a descriptor pair and therefore needs one. So the departure is recorded here and
    /// settled at the top of [`Self::stream`], which is the same frame step the arrivals are
    /// built in. The client has no such split: it releases
    /// on the spot — but it is the same edge at the same point in the frame.
    released_blocks: Vec<BlockDraw>,
    /// The streaming context: everything [`WorldScene::load`] read once and
    /// [`WorldScene::stream`] needs again for every block that enters the window.
    land: LandContext,
    terrain_key: PipelineKey,
    /// The landscape detail pass's state: [`Self::terrain_key`] with the separate detail
    /// pass's blend (`SRCALPHA`, `INVSRCALPHA`) and a less-or-equal depth test.
    terrain_detail_key: PipelineKey,
    /// Two light pools: the static pool is cleared and rebuilt from every
    /// visible cell; the dynamic pool is rebuilt whenever the viewer is set.
    light_pools: LightPools,
    /// What the static pool was last built for: the player's cell id and the resident block
    /// set. Retail rebuilds on the cell change alone; this build's blocks can arrive after
    /// the cell did, so a new resident block is a rebuild too (a superset of retail's
    /// rebuilds).
    static_pool_key: Option<(u32, Vec<(i32, i32)>)>,
    /// `character_parts[i]` draws `part_array.parts[i]`. Built once: nothing here
    /// swaps a part's `gfxobj_id`, because that is `ObjDesc` and comes from the server.
    character_parts: Vec<PartLevels>,
    /// The level each of the body's parts is drawing this frame. The local
    /// player is exempt from `get_degrade` (the viewer-distance update skips
    /// it for the player's own id), so with a local body this is all zeroes and stays so
    /// however far the chase camera pulls back; it exists so that the exemption is a value a
    /// test can read rather than an absence it has to infer.
    character_part_levels: Vec<u32>,
    /// The body's half of `SceneObject::part_draw_pos`. The local player is
    /// exempt from `get_degrade`, so his selected level is always 0 and his mode is always the
    /// mode of *level 0* — which is not necessarily 1, and is why this is computed rather than
    /// assumed to be `pos`.
    character_part_draw_pos: Vec<Frame>,
    /// The body's half of `SceneObject::part_cypt`. The player is exempt from
    /// *degrading*, not from being **sorted**: the viewer-distance update measures his distance
    /// before it decides whether to consult the record at all, and
    /// sorts him against everything else in the cell.
    character_part_cypt: Vec<f32>,
    /// A forced detail level, -1 unless a test pins it. See
    /// [`Self::set_force_level`] for why nothing in production writes it.
    force_level: i32,
    /// The `GfxObj` id each entry of [`Self::character_parts`] was **actually baked from**,
    /// recorded inside [`Self::build_part_meshes`] at the moment it read the array, like
    /// `PreviewObject::built_from`.
    ///
    /// This is not the same claim as `part_array.parts[i].gfxobj_id`: that is the *model*, and
    /// an `ObjDesc` (or a `set_setup_id`) applied after the bake leaves the two disagreeing
    /// while every model assertion still passes. A test that wants to say the body on the GPU
    /// is the body the server described has to read this.
    character_built_from: Vec<DataId>,
    /// The drawing half of every server object -- its part geometry and this
    /// frame's level choices -- keyed like `world.objects`, so both walk in id order.
    object_draws: BTreeMap<ObjectId, SceneObject>,
    /// Geometry per **appearance** — the setup record plus what the server's `ObjDesc` does to it.
    /// Two mosswarts cost one decode and one set of textures; two players in the same outfit do
    /// too, and two in different outfits do not.
    object_meshes: BTreeMap<AppearanceKey, Arc<Vec<PartLevels>>>,
    /// Geometry per setup for the placed statics that play a default animation, and whether it
    /// was built with the objects' look: every butterfly of a meadow shares one entry. An entry
    /// no host holds any more is released with the blocks that held it.
    host_meshes: BTreeMap<(DataId, bool), Arc<Vec<PartLevels>>>,
    /// One geometry entry per emitter graphics id in play: every particle of
    /// an emitter shares one graphics object, so a town full of torches costs one mesh.
    particle_gfx: ParticleGeometry,
    /// What the last draw drew. [`Self::draw`] takes `&self`, so its two counters cannot go
    /// in [`SceneStats`].
    frame_particles: std::cell::Cell<ParticleStats>,
    /// For the same reason: parts submitted and parts drawn through a cloned
    /// material's alpha channel during the last [`Self::draw`].
    frame_material_parts: std::cell::Cell<(u32, u32)>,
    /// Same bracket: what the object pass's alpha lists held on the last
    /// [`Self::draw`]. See [`AlphaListStats`].
    frame_alpha_lists: std::cell::Cell<AlphaListStats>,
    /// Same bracket: what the *landscape's* alpha list held and in
    /// what order it was drained on the last [`Self::draw`]. See [`LandscapeAlphaStats`].
    frame_landscape_alpha: std::cell::Cell<LandscapeAlphaStats>,
    /// The landscape's alpha queue itself: the blocks whose baked
    /// `blended` batches have been *queued* and not yet drawn, in the order the landscape pass
    /// walked them (far to near).
    ///
    /// It is frame state rather than a local because the flush is not issued by the pass that
    /// fills it. The world pass issues it after the landscape walk **and the objects that walk
    /// drew**; an indoor frame that sees outdoors through an opening issues it between that
    /// outdoor pass and the depth clear.
    frame_alpha_pending: std::cell::RefCell<Vec<(i32, i32)>>,
    /// The landscape's clip-list queue under "Multiple Pass Alpha": the blocks whose
    /// clip-mapped batches the landscape pass has drawn in place and whose second, blended
    /// pass is still owed, in the order they were drawn.
    ///
    /// Separate from [`Self::frame_alpha_pending`] because the two fill at different points:
    /// this build draws the alpha-tested batches in place after the whole block walk, so a
    /// building's own flush inside the walk has nothing on this list yet, and the frame's
    /// flush drains it first, clip list before alpha list.
    frame_multipass_pending: std::cell::RefCell<Vec<(i32, i32)>>,
    /// Same bracket: the alpha-list draws in device order. See [`AlphaDraw`].
    frame_alpha_order: std::cell::RefCell<Vec<AlphaDraw>>,
    /// The blended static batches queued for the next object pass's alpha list: the
    /// interior cells' (queued where cell drawing draws them) and, once that pass takes them,
    /// the landscape's still pending. See [`StaticBlendRef`].
    frame_static_blend: std::cell::RefCell<Vec<StaticBlendRef>>,
    /// Same bracket: the object pass's blend-list draws (parts and particles) in device
    /// order, each with the viewer distance it was sorted by.
    frame_blend_order: std::cell::RefCell<Vec<(AlphaDraw, f32)>>,
    /// What the per-object frustum test did on the last [`WorldScene::draw`].
    frame_object_cone: std::cell::Cell<ObjectConeStats>,
    /// The device's frame stamp as cell drawing reads it: a part of a cell static drawn with
    /// this stamp is not drawn again until it moves on, which it does at the start of each
    /// frame and between an indoor frame's outdoor pass and its interior one. See
    /// [`CellStaticPart::drawn`].
    frame_stamp: std::cell::Cell<u32>,
    /// [`Self::frame_stamp`] as the last draw began, so a probe can tell the parts that draw drew.
    frame_stamp_first: std::cell::Cell<u32>,
    /// The number of the last turn a cell's object draw began: one per offer of a cell's parts,
    /// so a cell given two turns in one frame (a room reached through two of a building's
    /// openings) draws at each only the parts that turn took. It only counts up.
    frame_turn: std::cell::Cell<u32>,
    /// Same bracket: every part run of a cell's static batches the last [`Self::draw`]
    /// submitted, in submission order. See [`CellStaticRunDraw`].
    frame_cell_runs: std::cell::RefCell<Vec<CellStaticRunDraw>>,
    /// What the per-part cell-object draw did on the last [`WorldScene::draw`].
    frame_cell_statics: std::cell::Cell<CellStaticDrawStats>,
    /// The bytes of a cell batch whose parts a cell's object draw draws only some of,
    /// gathered for the one submission. Reused from draw to draw.
    static_scratch: std::cell::RefCell<Vec<u8>>,
    /// Same bracket: one entry per subset of every part the last
    /// [`Self::draw`] submitted, in submission order. See [`PartSubsetDraw`].
    ///
    /// It is recorded *as the draw runs* rather than recomputed afterwards, deliberately: a
    /// probe that re-derived the order would agree with itself whatever the draw did, which is
    /// the failure `PartLevelProbe::would_choose` exists to keep separate from
    /// `PartLevelProbe::level`.
    frame_part_order: std::cell::RefCell<Vec<PartSubsetDraw>>,
    /// Every **interior** cell the last [`Self::draw`] actually walked:
    /// The `cell_draw_list` for an indoor viewer, plus the cells
    /// building drawing reached through a building's own portals for an outdoor one.
    ///
    /// `None` until a draw has run at all, which is not the same as an empty set: an empty set
    /// is "the cell walk reached nothing", a state a dungeon frame can genuinely be in, while
    /// `None` is "nobody has asked the cell walk anything yet" — every headless harness that
    /// never calls [`Self::draw`], and the frames before the first one. See
    /// [`Self::drawn_cells`], whose consumer treats the two differently on purpose.
    frame_drawn_cells: std::cell::RefCell<Option<std::collections::BTreeSet<u32>>>,
    /// Every object the last [`Self::draw`]'s object pass **offered**, i.e.
    /// every object at least one of whose parts survived phase, `NoDraw`, mesh and view-cone
    /// selection and reached [`draw_part`].
    ///
    /// This is the set offered to the selection ray, and it is not
    /// [`Self::frame_drawn_cells`]. Normal indoor rendering has no direct outdoor pass; its
    /// only outdoor pass comes from cell rendering and is skipped when
    /// `outside_view.view_count == 0`. In that state **no outdoor object is submitted**, so
    /// none reaches mesh drawing or the selection-ray offer. A cell set cannot express that
    /// absence because the skipped cells are outdoor cells outside the interior walk.
    ///
    /// Recorded *as the draw runs*, after the view-cone decision and before `draw_part`, for
    /// the reason [`Self::frame_part_order`] gives; accumulated across **both** passes of a
    /// split frame. It is deliberately **not** `drawn_part_order`: that records final device
    /// submissions, which is later than and therefore a stricter rule than native.
    ///
    /// `None` until a draw has run at all — every headless harness that never calls
    /// [`Self::draw`], and the frames before the first one — and it restricts nothing. An empty
    /// set is a real answer. See [`Self::drawn_objects`].
    frame_pick_candidates: std::cell::RefCell<Option<std::collections::BTreeSet<ObjectId>>>,
    /// The outside-view list's screen outlines for the last
    /// [`Self::draw`], in pixels with y down: the openings the indoor traversal actually
    /// found a hole through, and the polygons the outdoor object pass was cone-tested against.
    ///
    /// Empty is the honest answer on an outdoor frame, on an indoor frame facing a wall, and
    /// on the unclipped traversal (`SceneConfig::portal_clip` off), which has a *count* but no
    /// polygons. The distinction the consumers care about is
    /// `ConstructedView::outside_view_count`, which is published separately by
    /// [`Self::indoor_outside_view_count`]. Recorded from the walk itself; see
    /// [`Self::outside_view_polys`].
    frame_outside_views: std::cell::RefCell<Vec<dereth_world_render::cells::clip::ViewPoly>>,
    /// `outside_view.view_count` as the last [`Self::draw`] actually computed
    /// it, or `None` when that draw did not take the indoor path at all.
    ///
    /// This exists so that [`Self::indoor_outside_view_count`] does not *re-derive* the number
    /// from the unclipped traversal while the draw's own traversal is the
    /// clipped one. A probe that re-ran the walk would agree with itself whatever the draw did
    /// — the failure [`Self::frame_part_order`] exists to keep separate — and here it would
    /// have reported "the window is visible" on a frame that drew no outdoor pass at all.
    frame_outside_view_count: std::cell::Cell<Option<usize>>,
    /// The top portal view per interior cell the last
    /// [`Self::draw`]'s walk reached, in screen pixels with y down.
    ///
    /// The object loop installs the cell's own top portal view as the active portal list,
    /// so every part of that cell's objects is cone-tested against **these** polygons and not
    /// against the screen, the same rule `outside_view` follows for the outdoor pass. The stamp
    /// loop installs the same views one at a time.
    ///
    /// Empty is an outdoor frame or the unclipped traversal (`SceneConfig::portal_clip` off),
    /// which has a cell list but no polygons; a cell missing from the map is one the walk did
    /// not reach and is already excluded by [`Self::frame_drawn_cells`].
    frame_cell_views:
        std::cell::RefCell<BTreeMap<u32, Vec<dereth_world_render::cells::clip::ViewPoly>>>,
    /// `(stamps issued, openings whose clip left nothing)` for the last
    /// [`Self::draw`]'s `IndoorStep::PortalDepthStamps`.
    ///
    /// A portal depth stamp is issued once per opening and view-polygon pair for its own cell,
    /// so the first number is not the number of openings; the second is the
    /// pairs reduced below three vertices, which is the
    /// opening being wholly outside that view. `Gpu::portal_stamps` counts both masks across
    /// the whole frame and cannot separate the indoor half from building drawing's.
    frame_portal_stamps: std::cell::Cell<(u64, u64)>,
    /// Where the detail textures were last taken from ([`Self::detail_source_used`]).
    detail_from: DetailSource,
    /// The sky objects, and the clock that positions them.
    sky: Option<crate::sky::SkyScene>,
    /// The region whose sky draws, with its light and fog, when it is not the world's own
    /// (`[Render] Sky`); the scenery and everything else still read the world's.
    sky_region: Option<Box<dereth_assets::Region>>,
    /// The files that sky's objects are read from when they are not the world's.
    sky_store: Option<RetailDatStore>,
    /// The surface cache that sky's textures are held in when its files are not the world's,
    /// so that no id it shares with the world's own files can hand one a picture of the
    /// other's.
    sky_cache: Option<BakeCache>,
    /// The device fog state left by the environment tick: the
    /// `D3DRS_FOGENABLE` / `FOGCOLOR` / `FOGSTART` / `FOGEND` quartet, kept as state for the
    /// same reason the client keeps it on the device: it is written on the **tick**,
    /// not per frame, and every draw between two ticks sees the same
    /// numbers. [`Self::apply_fog`] is the writer.
    fog: dereth_render::camera::FogParams,
    /// Frame-rate estimate and degrade multiplier.
    pub degrade: DegradeState,
    /// Draw the landscape by splatting its layers rather than with its composites. See
    /// [`SceneConfig::terrain_splat`] and [`Self::toggle_terrain_splat`].
    terrain_splat: bool,
    /// Counters for the startup log line and for the tests.
    pub stats: SceneStats,
    /// The object id the view-cone check watches for the selection latch,
    /// as the selected-object setter last left it.
    ///
    /// A `Cell` because it is a *global* in the client — process-wide physics-part state, written by
    /// the selection path and read by the draw — and [`Self::draw`] takes `&self`. Threading a
    /// `&mut dereth_client_model::World` into the draw would have been the alternative, and it would have
    /// modelled a global as a field of the game world, which it is not: the physics-part state
    /// belongs below the game-object layer entirely. 0 is the client's own "nothing
    /// selected", and the draw tests it before the id compare (`viewcone_check_object_id != 0`).
    viewcone_check_object_id: std::cell::Cell<u32>,
    /// Drawing's `selected_object_in_view = 1`, observed for the frame
    /// and drained by `dereth_client_runtime::interaction::use_time` into
    /// the selection visibility latch.
    ///
    /// Only the *observation* lives here, not the latch: retail has one global and this build
    /// has a crate seam across the middle of it, so exactly one side has to own the memory.
    /// `dereth-client-model` owns it, because its only
    /// reader and it is in that crate.
    selected_part_drawn: std::cell::Cell<bool>,
}

/// One drawable piece of one animated part, prepared once and re-submitted every frame with
/// that part's own world matrix.
///
/// A static object is baked into world space at load ([`StaticBatch`]) because it never moves.
/// A character's parts move every frame, so the vertices stay in **object space** and the
/// transform rides in the `PerDraw` block, matching the client's submission
/// for every part of every object, every frame.
#[derive(Debug)]
pub(crate) struct PartMesh {
    pub(crate) key: PipelineKey,
    /// Authored surface type, before runtime flags are added.
    pub(crate) surface_type: u32,
    /// The same surface's state with material alpha enabled.
    ///
    /// Drawing a part first installs its current material's alpha flag.
    /// Surface setup then reads that as `SurfaceContext::material_has_alpha`,
    /// which is a *different* pipeline state, so a part carrying a translucent cloned
    /// material cannot be drawn with [`Self::key`]. Both keys come from the same
    /// [`PipelineKey::state_from_surface`] call site in [`resolve_surface`]; the draw picks.
    pub(crate) key_material_alpha: PipelineKey,
    /// The same surface's state with `SurfaceContext::force_alpha` set: the
    /// second pass "Multiple Pass Alpha" gives a clip-mapped subset when the alpha list is
    /// flushed. Blending `SRCALPHA / INVSRCALPHA` on, the alpha test off, the depth write
    /// off and the depth test still `LESS`, so it lands only where the first, alpha-tested
    /// pass left the depth alone: the soft edge texels the test cut away.
    pub(crate) key_force_alpha: PipelineKey,
    /// The same surface's state with `SurfaceContext::detail_in_stage1` set —
    /// changing only `StageOps::BASE` to `StageOps::SINGLE_PASS_DETAIL`.
    /// Mesh-subset drawing chooses between them from the current detail surface on each
    /// draw, so, like
    /// [`Self::key_material_alpha`], both come out of one
    /// [`PipelineKey::state_from_surface`] call site and the draw picks.
    pub(crate) key_detail: PipelineKey,
    pub(crate) alpha_ref: u8,
    pub(crate) texture: Option<TextureSlot>,
    /// 0 = linear/wrap, 1 = linear/clamp — `Gpu::bind_texture`'s table.
    pub(crate) sampler: u32,
    /// This subset's alpha classification, built from its authored surface type:
    /// 2 = Alpha, 8 = ClipMap, 4 = Translucent, 0 = opaque.
    ///
    /// Carried on the mesh rather than recomputed at draw time because the client builds it
    /// once, at mesh construction, from the surface the subset was collected under — and
    /// because it is **not** derivable from [`Self::key`]: surface setup gives a ClipMap
    /// subset `alpha_blend && alpha_test` and a Translucent one
    /// `alpha_blend && !alpha_test`, which is the same pair of flags an `Alpha` surface gets,
    /// so the pipeline state cannot tell mask 8 from mask 4 from mask 2.
    pub(crate) subset_mask: u32,
    /// The surface record this subset was collected under -- `GroupKey::surface`, i.e. the dat id
    /// [`Self::subset_mask`] and [`Self::key`] were both derived from.
    ///
    /// Carried so a test can re-read the record from `client_portal.dat` and re-derive the
    /// mask, rather than asserting that the scene agrees with itself. `None` for a group whose
    /// graphics object names no surface, which resolves to `RenderState::default()` and mask 0.
    pub(crate) surface: Option<DataId>,
    /// The surface's luminosity, which the subset draw writes into
    /// the bound material's `Emissive` when it is positive, whatever material is
    /// bound: a glowing surface glows on a part with a clone and on one without.
    pub(crate) luminosity: f32,
    /// The same 36-byte `XyzNormalDiffuseTex1` layout the static batches use
    /// ([`OBJECT_VERTEX_STRIDE`]).
    pub(crate) vertices: Vec<u8>,
}

/// One part's geometry, **one entry per degrade level**.
///
/// The client builds one graphics object per level of the part's degrade record, re-picks
/// the level per part per frame, and then draws the picked level's object. A part that
/// carried exactly one baked mesh — the degrade lookup's answer at `d = 0`, chosen at bake
/// time — would draw a creature 300 m away with its near-band mesh.
///
/// Unlike a baked static this needs no batching decision. A static's levels have to share one
/// world-space vertex pool because a block's triangles are merged by surface; a part is drawn
/// on its own with its own `PerDraw`, so a level is an **index** and switching one costs
/// nothing but the index.
#[derive(Debug)]
pub(crate) struct PartLevels {
    /// `levels[i]` is `gfxobj[i]`'s meshes. **Always at least one entry.** An entry is empty
    /// when that level draws nothing — `gfxobj_id == 0`, for which part drawing returns
    /// immediately — which is a real state and not a missing mesh: 4,131 shipped records end in a
    /// terminator whose selection means *draw nothing*.
    levels: Vec<Vec<PartMesh>>,
    /// The **level-0 graphics object** actually built -- the body's `built_from`,
    /// generalised to every part so that a test can re-read that
    /// object from `client_portal.dat` and check [`Self::sort_center`] against its own.
    ///
    /// It is not `PhysicsPart::gfxobj_id`: **229 of the 4,131 shipped
    /// `GfxObjDegradeInfo` records name a level-0 mesh that is not the object pointing at
    /// them**, and an `ObjDesc` applied after the bake moves the part's id and not this one.
    gfxobj: DataId,
    /// The graphics object's sort centre — the **level-0** mesh's, because
    /// the viewer-distance update reads it before it asks the degrade lookup anything, so the
    /// distance does not move when the level does.
    sort_center: Vec3,
    /// The part's degrade record, shared with every other part built from the same graphics
    /// object.
    /// `None` when the object names no record, or names one with a single level: neither can
    /// switch, so both are level 0 for ever.
    info: Option<Arc<dereth_assets::motion::GfxObjDegradeInfo>>,
    /// Each level's drawing sphere, **one per level**, in the part's own unscaled space.
    ///
    /// Mesh drawing is handed the selected level's graphics object
    /// and reads the drawing sphere off *that* object,
    /// so the cone tests the sphere of the level actually being drawn and not level 0's. It is
    /// a parallel `Vec` rather than a field on a per-level struct because [`Self::levels`] is
    /// already one, and the two are built in the same loop and indexed by the same
    /// `deg_level`.
    ///
    /// `None` for a level whose graphics object has no drawing BSP — the same state
    /// `WorldPicker::parts_without_drawing_sphere` counts. The view-cone check cannot be run on
    /// such a part at all; see `WorldScene::part_cone` for what this build does instead.
    spheres: Vec<Option<(Vec3, f32)>>,
    /// Whether this part was drawn from another era's look, whose surface cache holds its
    /// pictures' links ([`LandContext::objects`]); the world's cache holds them otherwise.
    from_look: bool,
}

impl PartLevels {
    /// A part with one level and no record.
    fn single(
        gfxobj: DataId,
        sort_center: Vec3,
        sphere: Option<(Vec3, f32)>,
        meshes: Vec<PartMesh>,
    ) -> Self {
        Self {
            levels: vec![meshes],
            gfxobj,
            sort_center,
            info: None,
            spheres: vec![sphere],
            from_look: false,
        }
    }

    /// `gfxobj[level]->drawing_sphere` — see [`Self::spheres`].
    ///
    /// A level past the end answers `None` for the same reason [`Self::at`] answers with no
    /// meshes: the client's `gfxobj[i] == NULL` draws nothing, and a part that draws nothing is
    /// never offered to mesh drawing at all.
    fn drawing_sphere_at(&self, level: u32) -> Option<(Vec3, f32)> {
        self.spheres.get(level as usize).copied().flatten()
    }

    /// What this part draws at `level`, with the same fallback the client has: none. A level
    /// past the end cannot happen (`get_degrade` returns an index into the record and the
    /// record is what built `levels`), and if it ever did, drawing nothing is the client's
    /// answer for a NULL `gfxobj[i]` and is the safe one.
    fn at(&self, level: u32) -> &[PartMesh] {
        self.levels.get(level as usize).map_or(&[], Vec::as_slice)
    }

    fn triangles_at(&self, level: u32) -> usize {
        self.at(level)
            .iter()
            .map(|m| m.vertices.len() / (OBJECT_VERTEX_STRIDE * 3))
            .sum()
    }

    fn triangles_held(&self) -> usize {
        self.levels
            .iter()
            .flatten()
            .map(|m| m.vertices.len() / (OBJECT_VERTEX_STRIDE * 3))
            .sum()
    }

    /// Every mesh of every level, for the upload reservation and for the tests.
    fn all(&self) -> impl Iterator<Item = &PartMesh> {
        self.levels.iter().flatten()
    }
}

/// One part of one moving object, as [`WorldScene::part_degrade_probe`] reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartLevelProbe {
    /// The server object it belongs to, or `None` for the **local body**.
    pub object: Option<ObjectId>,
    /// Its index in the object's part array.
    pub part: usize,
    /// Whether the physics-object id matches the viewer id — the one object the viewer-distance
    /// update never asks the degrade lookup about.
    pub is_player: bool,
    /// What was handed to the degrade lookup: the viewer distance divided by the graphics
    /// object's z scale. Published so a test can
    /// re-run the lookup against the record rather than trusting the answer.
    pub distance: f32,
    /// The viewer distance itself, before the z-scale divide.
    /// The depth sort sorts on this and not on
    /// [`Self::distance`], so a test asserting the draw order has to see the same number the
    /// sort saw.
    pub cypt: f32,
    /// The level-zero graphics object's sort centre, in part-local space: the
    /// first input from which the viewer-distance update measures [`Self::cypt`].
    pub sort_center: Vec3,
    /// The graphics-object scale, the second input. The sort centre is scaled
    /// by it before it goes through the part frame.
    pub gfxobj_scale: Vec3,
    /// The level-0 graphics object built, so a test can re-read it from
    /// the dat and check [`Self::sort_center`] rather than trusting the bake.
    pub gfxobj: DataId,
    /// The level this part is **drawing** — what the last
    /// `WorldScene::refresh_part_levels` stored.
    pub level: u32,
    /// What the selection would choose *now*, at this instant's camera. Equal to
    /// [`Self::level`] on any frame that has been stepped; the two are published separately
    /// so that "the scene agrees with itself" and "the scene agrees with the record" are
    /// different assertions.
    pub would_choose: u32,
    /// The `GfxObjDegradeInfo` this part carries, when it carries one.
    pub record: Option<DataId>,
    /// How many levels of geometry the bake is holding for it. 1 means it cannot switch.
    pub levels_held: usize,
    /// The `degrade_mode` of [`Self::level`], as `get_degrade`
    /// returned it beside the level.
    pub mode: dereth_world_render::objects::degrade::DegradeMode,
    /// The part's position frame, this frame's animation frame composed with
    /// the object frame.
    pub pos: Frame,
    /// The part's draw-position frame as the scene is **drawing** it, i.e.
    /// what `refresh_part_levels` stored and `draw_part` submitted. Equal to [`Self::pos`]
    /// exactly when the mode is `None` or the switch is clear.
    pub draw_pos: Frame,
    /// The `viewer_heading` the viewer-distance update measured, so a
    /// test can re-run `calc_draw_frame` on the same three inputs rather than assert that
    /// `draw_pos` merely differs from `pos`.
    pub viewer_heading: Vec3,
    /// The origin of the object this part belongs to, in the same space as the camera: the
    /// point the object-level viewer-distance update measures from.
    pub object_origin: Vec3,
    /// Whether the part was handed its land cell's or its object's distance and heading
    /// rather than measuring its own.
    pub shared: bool,
    /// Whether the part's object stands in a land cell rather than an interior one.
    pub outdoors: bool,
}

/// One baked static placement, as [`WorldScene::degrade_probe`] reports it.
/// Its `(distance, level, record)`, plus the billboarding half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticLevelProbe {
    /// What was handed to `get_degrade`: the viewer distance over the z scale. Published so a
    /// test can re-run the lookup against the record read back from the dat rather than
    /// trusting the answer the scene already has.
    pub distance: f32,
    /// The level this placement is drawing.
    pub level: u32,
    /// The `GfxObjDegradeInfo` it was asked.
    pub record: DataId,
    /// The `degrade_mode` of [`Self::level`], as `get_degrade` returned it
    /// beside the level.
    pub mode: dereth_world_render::objects::degrade::DegradeMode,
    /// Whether *any* level of the record billboards, i.e. whether this
    /// placement's vertices are baked part-local and re-transformed as they are assembled.
    /// Decided per record at bake time, so it does not move when the level does.
    pub billboards: bool,
    /// The part's position frame, block-local: the frame the bake placed the
    /// part at.
    pub pos: Frame,
    /// The part's draw-position frame: what
    /// `calc_draw_frame(pos, mode, viewer_heading)` produced, and the frame
    /// [`assemble_batches`] actually transformed the vertices by. Equal to [`Self::pos`]
    /// exactly when the mode is `None`.
    pub draw_pos: Frame,
    /// The `viewer_heading` the viewer-distance update measured, the
    /// third argument `calc_draw_frame` was given. Published so the assertion can be
    /// "`draw_pos` is what `calc_draw_frame` returns for *these* inputs" rather than
    /// "`draw_pos` differs from `pos`".
    pub viewer_heading: Vec3,
    /// Whether the placement was handed its land cell's or its object's distance and heading
    /// rather than measuring its own.
    pub shared: bool,
    /// The viewer distance the placement was measured at, before the z scale.
    pub cypt: f32,
    /// The owning object's origin, block-local; `None` for a building.
    pub object_origin: Option<Vec3>,
    /// The viewer, in the same block frame.
    pub viewer: Vec3,
    /// Whether the placement's object stands in a land cell rather than an interior one.
    pub outdoors: bool,
}

/// One object the server put in the world.
///
/// It is deliberately **not** a [`Character`](dereth_client_runtime::character::Character): a remote object has no local physics in this
/// build. The native client would integrate its velocity and collide it
/// against the landscape between server corrections; running that here would be dead reckoning,
/// and the client is a viewer. What is kept is the animation half — the
/// part array and the motion table — because playing back a motion the *server* chose is not a
/// prediction of anything.
///
/// This is only the drawing half; the simulation half is
/// [`WorldObject`](dereth_client_runtime::world_state::WorldObject), in `WorldState::objects` under the same id.
#[derive(Debug)]
struct SceneObject {
    /// Shared geometry, one entry per part of the setup. `parts[i]` draws
    /// `driver.part_array.parts[i]`. Each entry holds **every** degrade
    /// level, which is why it can still be shared: the levels are the same for two objects
    /// with the same appearance, and only the *choice* is per object.
    meshes: Arc<Vec<PartLevels>>,
    /// The setup the object is built on, so a change of look can build it again.
    setup: DataId,
    /// The appearance [`Self::meshes`] is shared under, so a change of look can build it
    /// again; `None` for the player's own object, which shares nothing.
    appearance: Option<AppearanceKey>,
    /// Which level each part is drawing **this frame** —
    /// one per part, re-picked by
    /// `WorldScene::refresh_part_levels`. Per object rather than per appearance
    /// because it is a function of this object's distance from the viewer.
    part_levels: Vec<u32>,
    /// The draw-position frame for each part this frame, i.e.
    /// `calc_draw_frame(pos, deg_mode, viewer_heading)`. Per object for the same
    /// reason [`Self::part_levels`] is: it is a function of this object's bearing from the
    /// viewer. Equal to the part's own `pos` for every part whose selected level is mode 1,
    /// which is 3,046 of the 3,234 parts measured across the shipped setups.
    part_draw_pos: Vec<Frame>,
    /// For each part this frame, the measured distance from the
    /// viewer to the part's sort centre.
    ///
    /// The *fourth* thing `WorldScene::refresh_part_levels` keeps out of the one
    /// viewer-distance update it already makes: the level per part, the billboard
    /// frame, and the distance itself, which
    /// `select_level` was already writing into `PartDraw::cypt` and both callers threw away.
    /// It is the part's depth-sort key, and without it the object
    /// pass draws in `BTreeMap<ObjectId, _>` order — which is id order, not depth order.
    part_cypt: Vec<f32>,
}

/// The scene's own sound stash, as the hook drain's sink. `dereth_client_runtime::audio::SoundTrigger`
/// is still this crate's, so the sink is the trait and this is its one implementor; the
/// newtype is the orphan rule's, not a design choice.
/// The frame simulation's counters, kept among the scene's own.
impl world_step::StepStatsSink for SceneStats {
    fn fold_steps(&mut self, d: dereth_client_runtime::object_step::ObjectStepStats) {
        self.remote_move_tos_failed += d.remote_move_tos_failed;
        if d.remote_move_tos_failed != 0 {
            self.remote_last_move_to_error = d.remote_last_move_to_error;
        }
        self.remote_target_updates += d.remote_target_updates;
        self.remote_sticks_pulled += d.remote_sticks_pulled;
    }
    fn updated(&mut self, without_a_sweep: bool) {
        if without_a_sweep {
            self.updates_without_a_sweep += 1;
        }
        self.updates += 1;
    }
    fn collision_script(&mut self, played: bool) {
        if played {
            self.collision_scripts_played += 1;
        } else {
            self.collision_scripts_unplayed += 1;
        }
    }
    fn held_without_holding_location(&mut self) {
        self.held_without_holding_location += 1;
    }
    fn server_objects_held(&mut self, n: usize) {
        self.server_objects_held = n;
    }
    fn hook_stats(&mut self) -> (&mut EtherealStats, &mut AttackStats) {
        (&mut self.ethereal, &mut self.attack)
    }
}

/// The scene's drawing half following object dispatch: the simulation is the world's, and
/// each of these runs at the point the old single pass did the same drawing work.
struct DrawFollows<'a> {
    draw: &'a mut SceneDraw,
    gpu: &'a mut Gpu,
    store: &'a Arc<RetailDatStore>,
}

impl world_objects::ObjectAppearance for DrawFollows<'_> {
    type Error = WorldError;

    fn removed(&mut self, _ws: &WorldState, id: ObjectId) {
        self.draw.object_draws.remove(&id);
    }

    fn removals_done(&mut self, ws: &WorldState) {
        self.draw.debug_check_object_halves(ws);
        // Object teardown releases each part's graphics objects and surfaces,
        // freeing them once the last link is gone. This
        // is that edge, run on the same frame as the removals that can reach it: an appearance
        // no `SceneObject` holds any more is dropped and its descriptor pairs handed back.
        //
        // Without it, `object_meshes` only ever grows, and a `long-solo-play` replay takes all 2,048
        // slots of a 2,048-slot heap at **t = 973.3 s of 1,098.5 s** — a ceiling on the
        // length of a session rather than on what is on screen. See
        // [`SceneDraw::release_unlinked_appearances`].
        let _ = self.draw.release_unlinked_appearances(self.gpu);
    }

    fn body_parts_changed(&mut self, ws: &WorldState) -> Result<(), WorldError> {
        let (array, setup) = match ws.character.as_ref() {
            Some(c) => (c.driver().part_array.parts.clone(), c.setup_id()),
            None => return Ok(()),
        };
        // The local player's own array: see `build_part_meshes`.
        let parts =
            self.draw
                .build_object_meshes(self.store, self.gpu, Some(setup), &array, true)?;
        self.draw.set_character_parts(self.gpu, parts);
        self.draw.stats.upload_bytes = self.draw.worst_case_upload_bytes();
        Ok(())
    }

    fn object_built(
        &mut self,
        _ws: &WorldState,
        id: ObjectId,
        setup_id: DataId,
        objdesc: &dereth_protocol::types::ObjDesc,
        driver: &MotionDriver,
        is_player: bool,
    ) -> Result<(), WorldError> {
        // The description's part swaps land in `gfxobj_id` and its texture-map swaps and
        // shift palette in the part appearance, and [`SceneDraw::build_part_meshes`] reads
        // both. Geometry can therefore not be keyed on the setup record alone — two
        // mosswarts still share one entry, but two players in different clothes do not. The
        // key is the *whole* recipe, so anything that dresses identically still shares.
        //
        // The player's own copy (only ever built with no local body) takes the degrade
        // exemption, which makes its meshes differ from another object with the same recipe,
        // so it does not share the recipe cache either.
        let key = AppearanceKey::new(setup_id, objdesc);
        let hit = if is_player {
            None
        } else {
            self.draw.object_meshes.get(&key).map(Arc::clone)
        };
        let meshes = match hit {
            Some(m) => m,
            None => {
                let m = self.draw.build_object_meshes(
                    self.store,
                    self.gpu,
                    Some(setup_id),
                    &driver.part_array.parts,
                    is_player,
                )?;
                let m = Arc::new(m);
                if !is_player {
                    self.draw.stats.appearance_builds += 1;
                    self.draw.object_meshes.insert(key.clone(), Arc::clone(&m));
                }
                m
            }
        };
        let part_levels = vec![0u32; meshes.len()];
        // The identity stands in until the first `refresh_part_levels`, which
        // runs before any submission.
        let part_draw_pos = vec![Frame::default(); meshes.len()];
        // On the same footing: zero until the first `refresh_part_levels`.
        let part_cypt = vec![0.0f32; meshes.len()];
        // The drawing half first, then the simulation half, under the same
        // id: both maps gain it together.
        self.draw.object_draws.insert(
            id,
            SceneObject {
                meshes,
                setup: setup_id,
                appearance: (!is_player).then_some(key),
                part_levels,
                part_draw_pos,
                part_cypt,
            },
        );
        Ok(())
    }

    fn creates_done(&mut self, ws: &WorldState) {
        self.draw.debug_check_object_halves(ws);
    }
}

/// What the scene actually loaded. Printed at startup and asserted on by the acceptance test,
/// because "it rendered" and "it rendered Dereth" are different claims.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneStats {
    pub blocks_meshed: usize,
    pub terrain_surfaces: usize,
    pub scenery_objects: usize,
    pub buildings: usize,
    pub static_objects: usize,
    pub object_batches: usize,
    /// Batches whose surface record did not resolve to pixels. Those draw with the vertex colour
    /// alone, which is opaque white, so a non-zero count here is visible as a white object.
    pub object_batches_untextured: usize,
    pub object_triangles: usize,
    /// Parts of the character that have drawing geometry, out of the setup's 34.
    pub character_parts: usize,
    /// Draws the character costs per frame: one per part per surface group.
    pub character_batches: usize,
    pub character_triangles: usize,
    /// Objects the server created that are currently drawable.
    pub server_objects: usize,
    /// Of those, how many have a motion table and therefore animate.
    pub server_objects_animated: usize,
    /// `0xF755`/`0xF754` triggers whose scripts were accepted and queued.
    ///
    /// It is a count of *queued scripts*, not of emitters: acceptance is reported as soon as
    /// the driver has taken the script, and a script whose hooks
    /// raise no particle-creation hook puts nothing on screen. Never assert particles with it.
    pub scripts_played: u64,
    /// Triggers `play_script` answered **0** for — no such object in the scene, no
    /// `PhysicsScriptTable` on it, or no row in the table at that intensity.
    ///
    /// Worth watching rather than ignoring: a non-zero here against a corpus replay is the
    /// signature of [`dereth_client_runtime::objects::Presence::phs_table`] not reaching the driver.
    pub scripts_unplayed: u64,
    /// Impacts that reached, found
    /// `SCRIPTED_COLLISION_PS` on the body, and whose `play_default_script`
    /// answered **1**. Same caveat as [`Self::scripts_played`]: queued scripts,
    /// not emitters.
    pub collision_scripts_played: u64,
    /// The same impacts answered **0** for — no such object in the scene, or no
    /// `PhysicsScriptTable` on it, which is the missing-table negative condition.
    ///
    /// A projectile whose setup carries no script table is the ordinary case, not a fault:
    /// the client still ran the whole receiver and still cleared the missile bits. Watch the
    /// *sum* of the two to know the receiver ran at all.
    pub collision_scripts_unplayed: u64,
    /// House-barrier contacts whose restriction script playback answered 1,
    /// meaning the sparks were queued.
    pub restriction_effects_played: u64,
    /// The same contacts that queued nothing: the house carries no physics script
    /// (the invalid script type, the no-script arm — the barrier still stops you), or the body
    /// has no `PhysicsScriptTable`. **The movement is unaffected either way.**
    pub restriction_effects_unplayed: u64,
    /// Contacts suppressed by the house-restriction-effects option.
    /// Counted separately from `unplayed` because the option is a *choice* and an empty
    /// script table is a *fault*, and one number cannot be read as both.
    pub restriction_effects_disabled: u64,
    /// `0xF74E Movement_VectorUpdate`s that applied velocity and angular velocity to a real
    /// body.
    pub vector_updates_applied: u64,
    /// Vector updates the object stream accepted and this scene had **no physics body** for.
    ///
    /// In the client every maintained object has one, so this counts a rebuild-only
    /// state: `prepare_object_physics` gives a body only to an animated, unparented,
    /// non-static object, so a vector update about a static or held object is accepted by the
    /// gate and then has nowhere to land. Counted rather than hidden, exactly as
    /// `held_without_holding_location` is — a non-zero value is a real divergence and not an
    /// error.
    ///
    /// **It counts only the case that is actually a loss.** A pair whose object is
    /// still in the table is put back rather than dropped (see
    /// [`dereth_client_runtime::objects::ObjectStream::park_vector_update`]) and counted in
    /// [`Self::vector_updates_deferred`]; this counts only the pair whose object left the
    /// table between the take and the put-back, which is the one way a `(velocity, omega)` can
    /// still be lost.
    pub vector_updates_without_body: u64,
    /// Vector updates put back for the next frame because the body did not exist **yet**.
    ///
    /// A create carries its own velocity through description setup, and this build spawns
    /// bodies in `ObjectStream::sync_physics`, which
    /// `App::sync_objects` runs **after** this drain — the reverse of
    /// the native order, which makes the body first and applies the description to it. So one
    /// deferral per create is the *expected* reading here and
    /// not a divergence; a value that keeps climbing for one object is.
    pub vector_updates_deferred: u64,
    /// Of those, how many are **held** — drawn on a holder's part rather than at a server
    /// position.
    pub server_objects_held: usize,
    /// Held objects whose holder's setup record carries no entry at the named `ParentLocation`.
    /// The holding-location lookup misses, `add_child` returns 0, and
    /// `set_parent` refuses the attachment, so the client does not draw
    /// them on the holder either.
    ///
    /// Cumulative and counted once per frame per object, because the refusal is re-derived
    /// every frame: a non-zero value means the scene holds an item its holder cannot hold, and
    /// **that object is not drawn**. Over the whole packet corpus this is zero — all 37 parent
    /// events find a holding location — so a non-zero value in play is news.
    pub held_without_holding_location: u64,
    /// Three counters, not one, so that *"the bit never arrived"* and *"the
    /// bit arrived and was clear"* cannot be confused.
    ///
    /// `nodraw_set` and `nodraw_cleared` are the no-draw setter firing, from either the
    /// object's own `NODRAW_PS` or a holder's `HIDDEN_PS` reaching it as a child;
    /// `hidden_changed` is hiding or unhiding firing at all. A scene where the server
    /// hides nothing reads `0, 0, 0` and a scene where the wire was never consulted reads the
    /// same, which is why the corpus test asserts the numbers rather than their sign.
    pub object_nodraw_set: u64,
    pub object_nodraw_cleared: u64,
    pub object_hidden_changed: u64,
    /// `0xF74B` state words carried to the **player's body** through
    /// `WorldScene::apply_character_state`. Separate from the three above because the body
    /// is the one object `apply_object_state` never sees, and a zero here with a non-zero
    /// `state_events` in [`dereth_client_runtime::objects::ObjectStream`] is exactly the defect of the word
    /// arriving and stopping in a table.
    pub character_states_applied: u64,
    /// Movement case 0 resolving a sticky target and applying the stick for a
    /// **remote** object.
    ///
    /// The server sends `MotionFlags & StickToObject` **46** times to remote objects across
    /// the seven captures, every one of them naming the session's own character; dropping
    /// them loses every stick. Split from [`Self::remote_sticks_unresolved`] so that
    /// *"nothing was stuck"* and *"the id named nothing this client knows about"* cannot be
    /// confused — the second is the stick's own object-lookup miss, which returns
    /// without touching the position manager.
    pub remote_sticks_applied: u64,
    pub remote_sticks_unresolved: u64,
    /// Frames on which the sticky manager moved a
    /// stuck remote object. Zero while a stick is recorded but has had no `TargetInfo` yet,
    /// which is the position manager's own `initialized` gate.
    pub remote_sticks_pulled: u64,
    /// Cases 6 through 9 reaching movement execution
    /// for a remote object. **83** buffers in the seven captures.
    pub remote_move_tos_performed: u64,
    /// `TargetInfo`s fed to a remote object's `MovementManager` and
    /// `PositionManager`; movement unpacking hands the same value to both after the 0.5 s
    /// gate.
    pub remote_target_updates: u64,
    /// Move-to cancellation reports from remote objects, and
    /// the last code one carried.
    pub remote_move_tos_failed: u64,
    pub remote_last_move_to_error: u32,
    /// Draws the server's objects cost per frame.
    pub server_object_batches: usize,
    pub server_object_triangles: usize,
    /// Distinct **appearances** whose geometry has been built: the setup record plus the server's
    /// `ObjDesc`. Objects that dress identically share one.
    ///
    /// This is the cache's *live* size. It is bounded by what is on
    /// screen rather than by the length of the session — see
    /// `WorldScene::release_unlinked_appearances`.
    pub server_object_setups: usize,
    /// Appearances **baked** over the whole session, counting a re-bake of an appearance that
    /// had been released as a second one. The denominator for
    /// [`Self::server_object_setups`]: the two would be equal by construction if nothing were
    /// ever released.
    pub appearance_builds: u64,
    /// Server-object and body builds with at least one part drawn with another era's look
    /// (`[Render] Objects`).
    pub object_appearances_from_look: u64,
    /// Builds that wanted another era's look and drew every part with the world's records.
    pub object_appearances_from_world: u64,
    /// Parts of those builds drawn from the other era's records.
    pub object_parts_from_look: u64,
    /// Parts of those builds drawn from the world's records: a model the other era does not
    /// hold or holds as another object, a picture or palette it lacks, or a texture change
    /// with no surface to go to.
    pub object_parts_from_world: u64,
    /// Appearances dropped because releasing the last link leaves them unowned.
    pub appearance_releases: u64,
    /// Texture descriptor slots handed back by those releases, one per `PartMesh` that carried
    /// one, through
    /// [`dereth_render::descriptor::TextureTable::release`].
    pub appearance_texture_releases: u64,
    /// Of those, the ones another owner still linked, so the slot stayed live. A non-zero value
    /// is the cache sharing a texture with the terrain bake or with another appearance and is
    /// **not** an error; a release that finds no entry at all is, and is counted by
    /// `TextureTableStats::unknown_releases`.
    pub appearance_textures_still_linked: u64,
    /// `object_meshes.len()` immediately after the last sweep — the complement of
    /// [`Self::appearance_releases`], so that "the sweep freed nothing" and "the sweep found
    /// nothing to free" cannot print the same number.
    pub appearances_still_linked: usize,
    /// Objects the server dressed with a non-empty `ObjDesc`, the player included.
    pub objdescs_applied: u64,
    /// Object-description application returning 0 — a part index the setup does not have, or a
    /// palette applied to an empty part array. A layout error, not bad input, which is why the
    /// acceptance test asserts on it.
    pub objdesc_failures: u64,
    /// The player's `ObjDesc` refused because the body could not be made to *be* the server's
    /// setup record, so its part indices would name different limbs.
    ///
    /// The body is rebuilt from the server's setup record first, and this counts only the case
    /// where that rebuild **failed** (a missing, undecodable or part-array-refusing setup).
    /// Three of the five in-world captures have players whose setup is not
    /// `ALUVIAN_MALE_SETUP` (the human female `0x0200004E`), so a body that is not rebuilt
    /// drops the whole description. A silent fallback to the wrong body is exactly what this
    /// counter exists to expose, and a counter nothing can read is a value nothing can assert.
    pub objdesc_setup_mismatch: u64,
    /// Sub-palette ranges refused, over every surface actually resolved.
    pub palette_range_failures: u32,
    /// Shift palettes whose base palette was missing from the dat.
    pub palette_missing: u32,
    /// Textures uploaded through the shared surface cache, i.e. descriptor-heap pairs consumed.
    /// The heap holds 32,768 pairs. Per-appearance geometry is what can make this grow
    /// with the population, and releasing unlinked appearances is what bounds it by the live
    /// set rather than by time.
    pub textures_uploaded: u32,
    /// Distinct surface groups this scene resolved through surface setup, and how
    /// many of them came back with a `curr_alpha` other than `0xFF` — i.e. how many draw
    /// *differently* now that the byte reaches the mesh.
    ///
    /// Two counters provide the denominator: a scene that resolved nothing at all otherwise
    /// reads identically to a scene whose every surface was opaque, and only the first of those
    /// is a broken measurement.
    pub surfaces_resolved: u32,
    pub surfaces_translucent: u32,
    /// Resolves served by the combined-texture cache's
    /// `AddRef` arm: a surface group whose (palette, texture) pair another group had already
    /// uploaded, costing a link rather than a descriptor pair. The difference between
    /// `surfaces_resolved` and `texture_key_hits` is roughly what the heap would hold for the
    /// same scene without the shared key.
    pub texture_key_hits: u32,
    /// The same for surface setup's 1x1 solid-colour texel, which is keyed in
    /// the same space and is a *different* transcription. The separate counters matter because
    /// this was learned from
    /// two surviving mutations: with the whole textured key forced to `UNCACHED`, a single
    /// combined total stayed comfortably non-zero off the solid-colour hits alone.
    pub solid_texel_key_hits: u32,
    /// [`BakeCache::solid_texels_uploaded`]: distinct **colour words** this
    /// session has taken a 1x1 texture for. Retail costs one texture for all of them; this
    /// build's memo costs one each, which is a *leak* rather than
    /// a deviation only if this count is unbounded. It is not: the colour word is a pure
    /// function of two fields of a surface record, so the ceiling is a shipped population.
    pub solid_texels_uploaded: u32,
    /// How many **distinct colour words** this session has seen, which is not
    /// the same number as [`Self::solid_texels_uploaded`]: a word whose last
    /// group departs is freed and re-uploaded when a block comes back, so the upload count
    /// rises with churn while this one cannot exceed the dat's own population.
    pub solid_colour_words: usize,
    /// Groups that shared a texture key with one that had expanded it under
    /// the other `BASE1_CLIPMAP` setting. The original's key excludes `bClipMap` too, so this
    /// is retail's behaviour; it is counted because it decides pixels by resolve order.
    pub clipmap_key_conflicts: u32,
    /// [`BakeCache::unowned_releases`] — a release naming a slot the shared surface memo does
    /// not own, i.e. a double release or a handle from another device. Tolerated and counted
    /// there; surfaced here so that the block and body release paths can
    /// be asserted at zero from outside the crate, on the same footing as
    /// `TextureTableStats::unknown_releases`.
    pub bake_unowned_releases: u32,
    /// Baked parts refused by the near-band detail guard -- the eleven
    /// designer markers the retail dat says are never drawn. Non-zero over any block that
    /// places one, and **zero** with the guard removed, which is what makes it a control
    /// rather than a comment.
    pub parts_not_drawn: u32,
    /// Resident baked parts whose `GfxObjDegradeInfo` has more than one level, and
    /// can therefore change mesh with distance.
    pub degrade_placements: usize,
    /// Of those, how many are drawing a level other than 0 **right now**.
    pub degrade_placements_degraded: usize,
    /// And how many are drawing **nothing** because the level they selected has
    /// `gfxobj_id == 0` -- past the last real band. Three counters preserve a third state:
    /// "held one level" and "held several
    /// and never left level 0" are different facts, and only the second says the wire works.
    pub degrade_placements_culled: usize,
    /// Cumulative level changes since the scene was built. Zero over a stationary camera and
    /// growing over a walk, which is the difference between a bake and a per-frame decision.
    ///
    /// **It counts the first selection too.** A placement is registered at level 0 and the
    /// first `refresh_degrade_levels` moves it to whatever the camera's distance says, so a
    /// freshly streamed block contributes one per placement that is not at level 0 -- the
    /// name admits two readings and this is the one it takes.
    pub degrade_switches: u64,
    /// The same three-state shape as [`Self::degrade_placements`]: resident
    /// placements baked **part-local**, i.e. able to turn, because their record names a
    /// billboarding mode at some level. Zero with [`SceneConfig::static_billboards`] clear,
    /// which is what makes that switch a control rather than a comment.
    pub degrade_placements_billboarding: usize,
    /// **The denominator**: resident placements whose *selected*
    /// level asks for a `degrade_mode` other than 1, counted from the mode `get_degrade`
    /// returned rather than from whether the bake can honour it -- so it reads the same in
    /// both arms of the switch, and the gap between the two counters is what a bake without
    /// billboarding loses.
    ///
    /// It is a different number from [`Self::degrade_placements_billboarding`] in both
    /// directions: a record can billboard at level 2 and not at level 0 (counted there, not
    /// here) and the switch can be off (counted here, not there).
    pub degrade_placements_billboarded: usize,
    /// Billboarded placements whose `draw_pos` **moved** on the last
    /// `WorldScene::refresh_degrade_levels`, i.e. billboards that turned this frame. Zero over
    /// a stationary camera, which is what keeps the assembly cost down.
    pub degrade_billboard_turns: usize,
    /// How many batch groups `WorldScene::refresh_degrade_levels` reassembled on
    /// the last frame -- one per block and one per interior cell whose selection moved. The
    /// number the level-selection design rests on: it is a double-digit fraction of the resident blocks
    /// over a walk, and 0 over a still camera.
    pub degrade_assemblies: usize,
    /// Object triangles **held** by the resident blocks, i.e. every level of every
    /// placement. [`Self::object_triangles`] is what one frame draws; this is what the bake
    /// costs in memory, and the ratio between them is the price of holding the levels.
    pub object_triangles_resident: usize,
    /// The same three-state shape as [`Self::degrade_placements`]: parts of
    /// **moving** things (creatures, server objects and the local body) whose graphics object names
    /// a `GfxObjDegradeInfo` with more than one level, so they can change mesh with distance.
    pub part_degrade_parts: usize,
    /// Of those, how many are drawing a level other than 0 **right now**.
    pub part_degrade_parts_degraded: usize,
    /// And how many of *those* are drawing **nothing**, because the level they selected has
    /// `gfxobj_id == 0`. "Holds several levels and never left level 0" and "holds one level"
    /// are different facts and only the first says the wire works.
    pub part_degrade_parts_culled: usize,
    /// Cumulative part-level changes since the scene was built. Zero over a stationary camera
    /// with nothing moving, and growing over a walk. **It counts the first selection too**,
    /// exactly as [`Self::degrade_switches`] does: a part is registered at level 0 and the
    /// first `WorldScene::refresh_part_levels` moves it to whatever the distance says.
    pub part_degrade_switches: u64,
    /// Parts of moving things whose *selected* level asks for a
    /// `degrade_mode` other than 1. Read off the mode `get_degrade` returned rather than off
    /// whether the switch can honour it, so it reads the same in both arms of
    /// [`SceneConfig::part_billboards`] and the switch cannot flatter itself. **188 of 3,234**
    /// over `first-login-walk-jump`; this is the live counter for it.
    pub part_billboards: usize,
    /// Of those, how many actually got a `draw_pos` different from their `pos` this frame —
    /// i.e. how many parts turned. Zero in the control arm by construction.
    pub part_billboards_turned: usize,
    /// Triangles **held** by the server's objects, i.e. every level of every part.
    /// [`Self::server_object_triangles`] is what one frame draws.
    pub server_object_triangles_resident: usize,
    /// The local body's half of the same pair.
    pub character_triangles_resident: usize,
    /// Sky objects present at the current time of day.
    pub sky_objects: usize,
    /// What the sky loaded and what it draws.
    pub sky_stats: crate::sky::SkyStats,
    /// What the particle path simulated and drew this frame.
    pub particles: ParticleStats,
    /// Placed statics and scenery whose setup record names a `default_script`, so they are live
    /// objects rather than only baked triangles.
    pub emitter_hosts: usize,
    /// Those of them whose setup names a default animation, drawn posed from their own part
    /// arrays rather than baked.
    pub animated_hosts: usize,
    /// How many times a host's default animation has advanced, over the session.
    pub hosts_animated: u64,
    /// Draw batches the resident interior cells' baked objects cost, and what
    /// static registration made of them on the physics side.
    pub cell_static_batches: usize,
    pub cell_statics: dereth_world_data::env_cells::CellStaticStats,
    /// Triangles the resident interior cells' baked objects **hold**, i.e. every
    /// level of every placement, the interior half of [`Self::object_triangles_resident`]:
    /// the resident figure for the furniture, beside the batch count.
    pub cell_static_triangles_resident: usize,
    /// What has handed back, cumulatively:
    /// blocks released, their interior cells, their sorted-cell building shells and the
    /// physics bodies their cells baked in.
    ///
    /// Four numbers rather than one, and cumulative rather than a residency, for this file's
    /// standing reason: a residency alone cannot tell "released the right things" from
    /// "released nothing and never grew". `blocks_released` is the denominator — it counts a
    /// block that turned out to hold no interior at all, so `cells_released == 0` over a walk
    /// with `blocks_released > 0` is a measurement and not a silence.
    pub blocks_released: u64,
    pub cells_released: u64,
    pub buildings_released: u64,
    pub cell_statics_destroyed: u64,
    /// Links a departed landblock's bake returned to
    /// [`BakeCache::release_group_texture`], and how many of those took an entry's count to
    /// zero and handed a descriptor pair back.
    ///
    /// Two numbers, and the pair is the point: a block whose every surface is still named by
    /// a block that stayed in the window releases many links and frees nothing, which is
    /// correct and reads identically to a release path that never ran if only the second is
    /// reported. A block dropped without releasing its links leaks them: the non-appearance
    /// half of [`BakeCache::surfaces`] then grows ~490 -> 964 slots over a 1,098.5 s
    /// `long-solo-play` replay.
    pub block_texture_releases: u64,
    pub block_textures_freed: u64,
    /// The same pair for the merged **terrain** surfaces, which are the other
    /// half of a departed block's slots and come from `LandContext::merge` rather than from
    /// [`BakeCache`]. Teardown returns one surface link per cell, so `terrain_surface_releases` is a count
    /// of *cells* torn down and `terrain_surfaces_freed` is a count of *surfaces* whose last
    /// cell that was.
    ///
    /// Both, and separately, because a walk whose release edge never ran leaves
    /// [`Self::terrain_unowned_releases`] at zero exactly as a walk that balanced perfectly
    /// does.
    pub terrain_surface_releases: u64,
    pub terrain_surfaces_freed: u64,
    /// `TerrainMergeCache::unowned_removes`: a removal naming a key the merge cache does not
    /// hold — a double release. Tolerated, counted and asserted at zero.
    pub terrain_unowned_releases: u32,
    /// `TerrainMergeCache::surfaces_built` and `surface_requests`: merged
    /// surfaces actually composited and uploaded, and the number of cells that asked.
    ///
    /// These are the counters *"is the merge cache being hit"* needs, and
    /// [`Self::terrain_surfaces`] is not: with a release edge the cache's
    /// residency is a function of **where the window is** and carries no hit-rate information
    /// at all.
    pub terrain_surfaces_built: u32,
    pub terrain_surface_requests: u64,
    /// Blocks that stayed resident and were re-meshed **in place**, keeping
    /// their object bake and replacing their terrain one. This is `build_slot`'s reuse arm,
    /// which is the second of the two sites a block's merged surfaces are returned at, and it
    /// is a site the object half of the same teardown does not have.
    ///
    /// Counted so that a test can say *the arm ran* rather than assume it from a configuration:
    /// `scenery_radius`, `land_radius` and the LOD ring table together decide whether it is
    /// reachable at all, and a test whose subject was never executed passes for the most boring
    /// possible reason.
    pub blocks_remeshed_in_place: u64,
    /// The same pair for the local body: links the **previous**
    /// `character_parts` returned when an appearance change replaced them.
    ///
    /// Appearance replacement first releases any geometry already attached to a part,
    /// so the old set's links go back on the same edge
    /// that installs the new one.
    pub body_texture_releases: u64,
    pub body_textures_freed: u64,
    /// The worst-case bytes one frame pushes through the dynamic upload ring.
    pub upload_bytes: usize,
    /// **The frozen viewpoint, made a number.**
    ///
    /// [`WorldScene::update`] calls, and how many ran with **no camera sweep**
    /// since the previous one.
    ///
    /// `Character::camera.viewer` has exactly two writers — the sweep and
    /// the viewer reset performed by `Character::teleport` — so a loop
    /// that drives `update` and never calls [`dereth_client_runtime::camera::update_viewer`] leaves the
    /// viewpoint wherever the last teleport put it, and `WorldScene::recenter` and
    /// `WorldScene::update_viewer_cell` then read a constant however far the body walks.
    /// A frozen viewpoint shows up only as a failure many frames downstream of the cause.
    ///
    /// **`update` is not a frame** — `App::frame` is `update` then the sweep, and the sweep is
    /// what moves the viewpoint — and this pair is what says so out loud.
    /// A running client holds `updates_without_a_sweep == 0` from the second update on,
    /// because the client's sweep is unconditional. The counter is deliberately **not** an
    /// assertion here: a harness that steps the body by teleporting is legitimate, and
    /// `Character::teleport` re-attaches the camera itself.
    ///
    /// A **bodiless** scene is not counted at all. There is no sweep without a
    /// body and the free camera *is* the viewpoint on that path, so "no sweep" is the
    /// correct state rather than a missing step — [`Self::updates`] is the denominator that
    /// keeps the two apart.
    pub updates_without_a_sweep: u64,
    /// [`WorldScene::update`] calls, ever — the denominator for
    /// [`Self::updates_without_a_sweep`], and the thing that tells "the sweep ran every time"
    /// from "the scene was never stepped".
    pub updates: u64,
    /// Ethereal-state hook events, counted at the seam.
    pub ethereal: EtherealStats,
    /// Attack hook events, counted at the seam.
    pub attack: AttackStats,
    /// What a landblock static's own hook events did.
    pub hosts: HostHookStats,
}

/// What the **landblock statics'** animation hooks did, cumulatively.
///
/// The client gives every static a full physics object, so a torch's hooks act on it
/// exactly as a creature's act on the creature: sound, ethereal state, scale and the rest. This
/// build's [`crate::particles::EmitterHost`] is that object **minus** the triangles the scene
/// bakes and **minus** the physics object the static registration owns — its own doc comment
/// says so — so only part of that set has anywhere to land. The hosts' queues must still be
/// drained, or they grow for as long as the session runs.
///
/// The fields are the landing sites, one each, so that "routed", "resolved before it got
/// here", "needs the object half this build's hosts do not have" and "has no receiver anywhere
/// in the client" are four different facts rather than one wildcard. Measured over the 2,161
/// retail setups that carry a `default_script` — the population these hosts are drawn from,
/// transitively through `CALL_PES` — the shipped data puts the whole
/// raisable set at `SOUND_TWEAKED` 524, `CREATE_PARTICLE` 7,757, `CALL_PES` 536,
/// `SOUND_TABLE` 49, `SCALE` 43 and `TEXTURE_VELOCITY` 11, and **zero** of `ETHEREAL`,
/// `NODRAW`, `SET_OMEGA`, `ATTACK` and `DEFAULT_SCRIPT_PART`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostHookStats {
    /// Every event taken off a host's queue, whatever became of it. The denominator, and the
    /// number whose *growth* would be the leak.
    pub drained: u64,
    /// Sound and tweaked-sound hooks → [`dereth_client_runtime::audio::SoundTrigger::Wave`] at the host's
    /// own world position. **The one that was not merely lost but audible:** 302 of the 2,161
    /// scripted setups put a sound in their default script — a torch's crackle, a fountain's
    /// splash, a brazier's roar — and this build played none of them.
    pub sounds: u64,
    /// Sound-table hooks resolve through the host setup's default sound table, using the same
    /// [`dereth_client_runtime::audio::SoundTrigger::Table`] route a server object's hooks take.
    /// The default sound table is carried on [`crate::particles::EmitterHost`].
    pub sound_types: u64,
    /// Sound-table hooks on a host whose setup names **no** default sound table,
    /// which the client cannot resolve either.
    ///
    /// Four scripted setups name a table at all. **Crossed with the setups that actually raise
    /// the hook, it is one**: of the 49 shipped `SOUND_TABLE` hooks exactly one is on a setup with a
    /// table (setup `0x0200171D` -> `0x200000CA`). The other 48 are unresolvable data.
    pub sound_types_no_table: u64,
    /// The particle trio and `CallPes`, both of whose arms [`dereth_animation`] resolves inside the
    /// driver. These are reports, not requests; draining them is the whole of the work.
    pub applied_in_driver: u64,
    /// `SetEthereal`, `SetNoDraw`'s state bit, `SetOmega`, `Attack` and `PlayDefaultScript` —
    /// every one of them a call on the physics object a host has not got, or on the child list
    /// that lives on it. Not a success, and not silently one: without this field a build that
    /// routed nothing would read exactly like one that routed everything.
    ///
    /// **`SetScale` is not in this set** — see [`Self::scale_hooks`]. The five
    /// here are raised by **nothing** in the shipped data, so this counter
    /// is expected to stay at zero and a non-zero reading is new data, not a new drop.
    pub no_body: u64,
    /// `SetScale` events drained.
    ///
    /// The scale hook carries `(end, time)`, whose immediate arm writes the part-array
    /// scale — which [`dereth_animation`] already did inside the driver — and
    /// `scale` on the object. That second
    /// store is what the pairing above made reachable, and it is not cosmetic:
    /// `transition`, `check_collision` and `SetPosition` each
    /// pass `scale` to collision as the scale its spheres are swept at.
    pub scale_hooks: u64,
    /// Of those, the ones whose host still had no body — a placement that produced none at
    /// all, or a block whose statics are not registered because the scene has no character.
    /// The third state, in [`EtherealStats`]' own terms: without it a build that routed
    /// nothing would read exactly like one that routed everything.
    pub scale_hooks_no_body: u64,
    /// `SetLights` and `SetTextureVelocity`, which have no receiver **anywhere** in this
    /// client — the inbound audit counts 1 and 11 shipped records for those rows. Distinct
    /// from [`Self::no_body`] because the missing half
    /// is a whole subsystem and not this object's.
    pub unreceivable: u64,
    /// `ReplaceObjectNoOp`. Retail's replace-object hook has no implementation,
    /// so retail does nothing here either.
    pub retail_noop: u64,
    /// `MotionDone` — a host has no motion interpolation to tell. Zero in play: a static runs a
    /// script, never a motion.
    pub motion_done: u64,
    /// `CallPes` events drained, and of those the ones that armed a timer rather than playing
    /// at once: the delayed-hook arm, seen from the statics. Counted separately from
    /// [`Self::applied_in_driver`] because a delayed hook that re-arms is how an undrained
    /// queue grows, and because a run where the timers stopped firing after the drain
    /// would otherwise be indistinguishable from one where they kept firing.
    pub pes_hooks: u64,
    /// Of those, the delayed ones.
    pub pes_hooks_delayed: u64,
}

/// One landblock static that runs a script, paired with its collision body.
///
/// The client uses one object for both halves: the part array whose script queues a
/// torch's flame and the physics body registered in the cell. Both indoor and outdoor
/// static-object lists store that same object. This build makes the two halves
/// in two modules, so this is the join.
#[derive(Debug, Clone, Copy)]
pub struct HostBody {
    /// The placement's authored cell.
    pub cell: CellId,
    /// Its setup id.
    pub setup: DataId,
    /// The host's own origin, absolute world space.
    pub at: Vec3,
    /// `static_objects[i]`, or `None` for a placement that produced no body.
    pub body: Option<dereth_physics::PhysHandle>,
    /// That body's `position.frame.origin`, brought into the same absolute space as
    /// [`Self::at`]. Equal to it, exactly, when the pairing is right.
    pub body_at: Option<Vec3>,
    /// That body's `position.objcell_id`.
    pub body_cell: Option<CellId>,
    /// The body's current scale, written by scale hooks.
    pub scale: Option<f32>,
    /// The body's collision radius, multiplied by `scale` for collision tests.
    pub radius: Option<f32>,
    /// The default sound table, if the setup names one.
    pub sound_table: Option<DataId>,
}

pub use dereth_client_runtime::anim_hooks::AttackStats;

pub use dereth_client_runtime::anim_hooks::EtherealStats;

/// What building one landblock needs, kept so a block entering the window can be built
/// without re-reading the region.
///
/// [`LandContext`] loads the region exactly once and every landblock generated
/// afterwards reads it; the same is true of the merged-surface cache
/// and decoded object geometry. The block builder gets both from their shared
/// caches rather than reading them again from disk.
struct LandContext {
    region: Box<dereth_assets::Region>,
    /// The region whose land surface draws the ground, when it is not [`Self::region`]'s: the
    /// world's region with another era's land surface in it (`[Render] Ground`). Mesh
    /// generation, the cell keys and the surfaces read it; scenery and everything else read
    /// [`Self::region`].
    ground: Option<Box<dereth_assets::Region>>,
    /// The files the ground's textures are read from when they are not the world's: another
    /// era's, whose texture ids the world's own files may hold different pictures under.
    ground_store: Option<RetailDatStore>,
    /// The terrain types the ground's land surface has a picture for, one bit each. A vertex
    /// of any other type is drawn as its neighbours are
    /// ([`dereth_terrain::land::fill`]).
    drawn: u32,
    tex_merge: dereth_assets::region::TexMerge,
    /// The ground's palette-shift land surface (an older dat set's software region), whose
    /// cells are composed here on the CPU; `None` for texture merging. Which of the two
    /// draws is read from the region's own land-surface record, never from the dat set's era.
    pal_shift: Option<dereth_assets::region::PalShift>,
    table: Box<[f32; dereth_terrain::consts::LAND_HEIGHT_TABLE_LEN]>,
    lighting: LandscapeLighting,
    /// The landscape surface cache, shared by every block: two neighbouring blocks with the
    /// same terrain pair merge to the same texture and must not upload it twice.
    merge: TerrainMergeCache,
    /// The decoded terrain tiles and alpha maps the merges are composited from, shared by
    /// every block. A handful of distinct images feed every merge in the window; decoding them
    /// per block instead was most of a landscape load. Emptied with the graphics resources,
    /// because the texture-detail preference changes what a decode produces.
    sources: HashMap<DataId, Option<Arc<Bgra8>>>,
    /// Compose with the device when it can ([`SceneConfig::gpu_terrain_merge`]).
    gpu_merge: bool,
    /// Those sources resident on the device for its compositor, by dat id. `None` is a source
    /// the dat does not carry. Emptied with [`Self::sources`].
    gpu_merge_sources: HashMap<DataId, Option<MergeSource>>,
    /// Builds compressed textures' mip chains off the main thread for the landscape's bakes;
    /// see [`crate::mip_worker`]. Asked only when [`Self::defer_mips`] is set.
    mip_worker: crate::mip_worker::MipWorker,
    /// Whether a bake may leave textures pending on the worker: set with a streaming budget,
    /// that is on a connected client. Offline runs, captures and the tests bake synchronously.
    defer_mips: bool,
    /// Whether the device can splat; set at load. Without it no splat data is made.
    splat_supported: bool,
    /// Whether blocks built now get splat layers, and whether they get composites. Exactly one
    /// is set: splat layers while the landscape is splatted, composites otherwise (and always
    /// on a device that cannot splat). Blocks built before a switch are converted by
    /// `WorldScene::convert_terrain`.
    splat_wanted: bool,
    composites_wanted: bool,
    /// The same sources as ordinary textures, for the splat draw. One link each, held here.
    splat_sources: HashMap<DataId, Option<TextureSlot>>,
    /// Each merge key's splat layers, made once.
    splats: HashMap<MergeKey, Arc<TerrainSplat>>,
    /// Decoded setup part lists, graphics-object triangulations and resolved surfaces, shared by
    /// every block for the same reason.
    bake: BakeCache,
    /// `[Render] Objects` when it is not the world's own: the other era's files the objects'
    /// parts, the scenery, the buildings and the statics are drawn from, and the surface cache
    /// their pictures are held in. `None`: the world's own.
    objects: Option<ObjectLook>,
    /// The verdicts the other era's look is drawn with: handed in by the application once
    /// it has them ([`SceneDraw::offer_object_identity`]), or worked out the first time the
    /// look was drawn, and kept when the look is switched off so switching it back does not
    /// work them out again.
    identity: Option<Arc<dereth_client_runtime::object_identity::ObjectIdentity>>,
    /// The environment-cell reader for the **draw** side. Physics has
    /// its own inside [`dereth_world_data::land_source::DatLandSource`], because that one has to be
    /// reachable from a `LandSource` behind an `Arc` and this one has to be reachable from the
    /// scene; they read the same records and share nothing else.
    cells: dereth_world_data::env_cells::EnvCellLoader,
    /// The interior statics' collision halves, by setup id, memoised for the session: the
    /// geometry that decides which cells each static is registered in, and so which cells draw
    /// it. `None` is cached too.
    // ORDER-OK: a decode memo, only ever looked up.
    static_geometry: HashMap<DataId, Option<Arc<dereth_physics::SetupGeometry>>>,
    /// The cell BSPs that search reads, by cell structure, memoised for the session.
    cell_bsps: dereth_world_data::env_cells::CellBspMemo,
}

/// The world's objects drawn with another era's look (`[Render] Objects`).
///
/// Every server object keeps the world's setup: its part list, how the parts are joined and
/// placed, and the world's motion data that moves them (the eras' setups differ in part count,
/// and an animation addresses parts by index). Each part then draws wholly from one era: from
/// [`Self::files`], the other era's portal alone, when [`Self::identity`] says the other era's
/// model of that id is the same object and everything the part's description names is there
/// ([`dereth_client_runtime::models::parts_for_look`]); from the world's records otherwise.
/// Never a mix within a part: the eras number surfaces differently, so a model of one era
/// read with the other's surfaces is painted with unrelated records.
///
/// The landscape's own objects (scenery, building shells, statics) are drawn whole, setups
/// included, from one era each by the same verdicts; a building shell takes the look only
/// where every vertex of it is where the world has it, so its doorways still meet the world's
/// interiors.
///
/// An interior cell draws from the other era's record of it, room by room, where that record
/// is the same room in the same place
/// ([`dereth_client_runtime::object_identity::ObjectIdentity::same_room`]) and the cell's
/// building shell takes the look too: its geometry and surfaces from [`Self::interiors`], and
/// the world's furniture each piece by the objects' verdicts. The world's record stays what the portals between
/// cells, collision and the simulation read.
pub(crate) struct ObjectLook {
    /// [`RetailDatStore::object_files`] of the world's store.
    pub(crate) files: Arc<RetailDatStore>,
    /// [`RetailDatStore::interior_files`] of the world's store: the other era's portal and
    /// cell files. `None` when that cell file is not beside the world, and every interior is
    /// the world's.
    pub(crate) interiors: Option<Arc<RetailDatStore>>,
    /// The reader of [`Self::interiors`]' cell records and the environments they name.
    cells: dereth_world_data::env_cells::EnvCellLoader,
    /// Which ids the look's records stand for the same object as the world's.
    pub(crate) identity: Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
    /// The surfaces those records resolve to. Apart from the world's cache because the two
    /// eras hold different records under the same ids, so a memo keyed by id would hand one
    /// era's surface to the other.
    pub(crate) cache: BakeCache,
}

/// The region a dat set from before Throne of Destiny carries for drawing with 3D hardware,
/// beside the one at `0x13000000` for drawing in software. The two differ only in their
/// sky and their land surface; a later dat set has only the one.
pub const HARDWARE_REGION: DataId = DataId(0x130F_0000);

/// Why a landscape style could not be drawn.
#[derive(Debug)]
pub enum StyleError {
    /// Its files are not present: the world's own are the other era's, and none of its era
    /// were given beside them.
    Missing(RequiredFiles),
    /// Its region would not read or decode.
    World(WorldError),
}

/// A landscape style's region and the files its pictures and objects are read from; `None`
/// files are the world's own.
#[derive(Debug)]
pub struct StyleSource {
    pub region: dereth_assets::Region,
    pub files: Option<RetailDatStore>,
}

/// Which region the detail textures are read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailSource {
    /// The drawn ground style's region.
    Ground,
    /// The world's own region.
    World,
    /// The end-of-retail files' region, when neither of the others names any.
    EndOfRetail,
}

/// The ground a style draws over the world's region.
#[derive(Debug)]
struct GroundChoice {
    /// The world's region with the style's land surface in it; `None` for the world's own.
    ground: Option<dereth_assets::Region>,
    /// The files its pictures are read from; `None` for the world's.
    files: Option<RetailDatStore>,
    /// The terrain types it draws with a picture of its own
    /// ([`dereth_terrain::land::fill::drawn_terrain_types`]).
    drawn: u32,
}

/// The three memos an [`ObjectBaker`] would otherwise hold for the length of one bake, kept for
/// the length of the session because a block can enter the window at any time.
pub(crate) struct BakeCache {
    // ORDER-OK: a decode memo, only ever looked up.
    parts: HashMap<DataId, Vec<dereth_client_runtime::models::ModelPart>>,
    // ORDER-OK: as above.
    geometry: HashMap<DataId, Vec<dereth_client_runtime::models::SurfaceGroup>>,
    /// Whether a surface record's type blends (`Alpha`, `InvAlpha`, `Additive` or
    /// `Translucent`): the surfaces whose baked batches are kept one per placed object, so the
    /// alpha list can draw each at its own distance. See [`BatchKey::instance`].
    // ORDER-OK: a decode memo, only ever looked up.
    surface_blends: HashMap<DataId, bool>,
    /// The scene-texture loader's answer per group. Uploading a tree's bark
    /// texture once per landblock instead of once per session is what this stops.
    // ORDER-OK: as above.
    surfaces: HashMap<GroupKey, ResolvedSurface>,
    /// **For the textures this memo owns:** how many live consumers hold
    /// each descriptor slot.
    ///
    /// Every [`Self::resolve`] hands out one link, hit or miss, exactly as the client
    /// increments a texture reference on a cache hit; every consumer that
    /// goes away returns one through [`Self::release_group_texture`]. At zero the entry leaves
    /// the memo and its descriptor pair goes back, which mirrors the texture object
    /// removing itself from `texture_table`.
    ///
    /// Without a count and a release, a `GroupKey` resolved once stays
    /// resident for the life of the scene, and 2,029 of them exhaust the 2,048-slot
    /// heap 973 s into a `long-solo-play` replay.
    ///
    /// **The count is per slot, not per `GroupKey`.** A per-group count would be exact only
    /// while every miss is a fresh [`dereth_render::descriptor::UNCACHED`] upload, because one
    /// slot then belongs to exactly one group. [`resolve_surface`] uses the combined-texture
    /// key, so two groups naming the same (palette, texture) pair
    /// share one image texture — and the link count belongs to the **data object**, i.e. the
    /// texture, not to the surface group that asked for one. So the count lives where the
    /// original keeps it, and the entries that name a slot live and die together with it.
    // ORDER-OK: keyed by descriptor slot and only ever looked up.
    links: HashMap<u32, u32>,
    /// Which memo entries name each owned descriptor slot, so a consumer can release with the
    /// `TextureSlot` its `PartMesh` kept rather than having to carry a whole `GroupKey` per
    /// mesh, and so that the last release can take every entry that named it out of
    /// [`Self::surfaces`].
    // ORDER-OK: keyed by descriptor slot; the `Vec` is drained wholesale on release.
    by_slot: HashMap<u32, Vec<GroupKey>>,
    /// The `BASE1_CLIPMAP` bit each owned texture key was first expanded with, for
    /// [`Self::clipmap_key_conflicts`] alone.
    // ORDER-OK: keyed by texture key and only ever looked up.
    key_clip_map: HashMap<dereth_render::TextureKey, bool>,
    /// Whether a texture id is palettised, so an `ObjDesc` palette can be dropped from the key
    /// for every surface it cannot change.
    // ORDER-OK: a decode memo, only ever looked up.
    palettised: HashMap<DataId, bool>,
    /// [`dereth_client_runtime::models::draws_at_near_band`] per graphics object. A part whose
    /// `GfxObjDegradeInfo` selects a level with no geometry is not drawn at all, and the
    /// answer costs two dat reads per distinct part without this.
    // ORDER-OK: a decode memo, only ever looked up.
    draws: HashMap<DataId, bool>,
    /// The `GfxObjDegradeInfo` a graphics object names, decoded once and shared by every
    /// placement of it. A landblock places the same tree hundreds of times.
    // ORDER-OK: a decode memo, only ever looked up.
    degrades: HashMap<DataId, Option<Arc<dereth_assets::motion::GfxObjDegradeInfo>>>,
    /// The level-zero sort centre, the point the viewer distance is measured to.
    // ORDER-OK: a decode memo, only ever looked up.
    sort_centers: HashMap<DataId, Vec3>,
    /// The drawing sphere per mesh id, the cone's half of the same lookup; see
    /// [`Self::drawing_sphere`].
    drawing_spheres: HashMap<DataId, Option<(Vec3, f32)>>,
    /// Every level's drawing sphere of a part's graphics object, with its degrade record or
    /// without one; see [`CellStaticPart::spheres`].
    // ORDER-OK: a decode memo, only ever looked up.
    level_spheres: HashMap<(DataId, bool), LevelSpheres>,
    /// Parts refused by that guard, summed over every bake this cache served.
    /// **Asserted on** rather than merely printed: an object silently swapped for nothing
    /// looks exactly like an object that was never there.
    pub(crate) parts_not_drawn: u32,
    /// Sub-palette ranges refused, summed over every surface actually
    /// resolved. **A tolerant lookup with a counter nothing compares lets real gaps hide
    /// in a green suite**; a test asserts this is zero over the whole
    /// corpus.
    pub(crate) palette_range_failures: u32,
    /// Shift palettes whose **base** palette was not in the dat, same discipline.
    pub(crate) palette_missing: u32,
    /// The image scale shared by clip-map, RGBA, and indexed textures. The preference poll
    /// derives it from `Render.EnvironmentTextureDetail`, and texture creation uses the
    /// resulting value: the value the *live* preference holds, re-read by
    /// [`WorldScene::update_from_preferences`] on every frame the option moves.
    pub(crate) image_scale: dereth_render::texture::ImageScale,
    /// How many surface resolves were left pending on the mip worker, ever. A bake compares it
    /// before and after to learn whether it has to be taken again.
    pub(crate) pending_surfaces: u64,
    /// `Render.EnvironmentTextureDetail` itself, for texture resolution's
    /// choice between a two-level `SurfaceTexture`'s high-res and original art through
    /// [`TextureStore::with_environment_texture_detail`].
    environment_texture_detail: u32,
    /// `(width, height, scale)` of the last texture this cache actually uploaded. Not a client
    /// field — the same instrument, and for the same reason, as
    /// `TerrainMergeCache::last_built`: `Render.EnvironmentTextureDetail`'s observable effect
    /// is the extent of the image that reaches the device, and a test that could only read
    /// [`Self::image_scale`] back would be measuring the variable.
    pub(crate) last_texture_built: Option<(u32, u32, u32)>,
    /// Cumulative uploads through [`resolve_surface`], the **texels** they
    /// carried, and how many of them [`dereth_render::texture::scale_surface`] actually shrank.
    ///
    /// `last_texture_built` alone cannot carry the claim: the *last* surface of a rebuild is
    /// whichever one the bake happened to
    /// reach last, and if that one is block-compressed it comes back at its source extent (see
    /// `scale_surface`'s declared departure) and the instrument reads "full resolution" on a
    /// run that scaled everything else. A population is the honest measurement, and
    /// `texels / uploads` is the number that moves.
    pub(crate) texture_uploads: u64,
    pub(crate) texture_upload_texels: u64,
    /// The same sum taken at the **source** extent. At `FULL_RES` the two are equal by
    /// construction, which is what makes their difference a measurement of the scale and not
    /// of the population.
    pub(crate) texture_source_texels: u64,
    pub(crate) textures_downscaled: u64,
    /// Textures uploaded through this cache, i.e. descriptor-heap pairs consumed. This is the
    /// count of pairs **currently held**: a released entry decrements it.
    pub(crate) textures_uploaded: u32,
    /// [`Self::release_group_texture`] naming a slot this cache does not own — a double
    /// release, or a handle from another device. Tolerated, counted, and asserted zero, on the
    /// same footing as `TextureTableStats::unknown_releases`.
    pub(crate) unowned_releases: u32,
    /// Resolves that took an `AddRef` on a texture this memo already held rather than
    /// uploading a second copy of it — the keyed texture cache's
    /// *"if the key is non-zero and already in the texture table, take a reference and return
    /// it"* arm. This is the number that says the keyed upload is doing anything at all, and a
    /// zero here would mean the key was wired and never hit.
    ///
    /// **Textured surfaces only.** The 1x1 solid-colour texel is keyed in the same space and
    /// counted separately in [`Self::solid_texel_key_hits`], because one total over both
    /// cannot fail when either is unwired.
    pub(crate) texture_key_hits: u32,
    /// The same, for surface setup's solid-colour texel.
    pub(crate) solid_texel_key_hits: u32,
    /// **The constant-colour cache counter.** Distinct colour words this
    /// cache has taken a 1x1 texture for; a hit on
    /// [`Self::solid_texel_key_hits`] does not increment it.
    ///
    /// Retail costs **one** texture here for the whole world: surface setup
    /// rewrites a single device-wide texel before
    /// each draw. This build bakes the slot into the batch and
    /// cannot rewrite a texel per draw, so it memoises one 1x1 texture per distinct colour
    /// word. That is pixel-identical and it is a deviation, and a deviation with an unbounded
    /// count would be a leak instead. This is the number that says which.
    ///
    /// **It is bounded, and structurally rather than by measurement.**
    /// `PipelineKey::state_from_surface`'s solid colour is
    /// `Some((alpha_from_translucency(s.translucency) << 24) | (s.color_value & 0x00FF_FFFF))`
    /// — a pure function of two fields of the surface record and of **nothing** else: not
    /// of the context, not of the polygon flags, not of the time of day, not of anything a
    /// player does. So the whole game can produce no more distinct colour words than there are
    /// distinct `(translucency, color_value)` pairs among the untextured surface records in
    /// `client_portal.dat`, which is a shipped, static population.
    /// A test counts it over the dat and measures what a session actually reaches.
    pub(crate) solid_texels_uploaded: u32,
    /// **The distinct colour words themselves, which is a different number from
    /// [`Self::solid_texels_uploaded`].**
    ///
    /// Uploads count *events*: the terrain and the departed blocks have a
    /// release edge, so a colour word whose last group leaves is freed and uploaded again when a
    /// block comes back, so the upload count rises with churn and overtakes the number of
    /// distinct words. Measured on the 24-station walk: **218 uploads of 124 distinct words**,
    /// against a whole-dat ceiling of **146**. A test that read the upload counter and
    /// called it "distinct colour words" would report a session exceeding a bound that
    /// cannot be exceeded.
    ///
    /// A set on a cache is normally the shape of a leak, so the bound is
    /// stated rather than assumed: `PipelineKey::state_from_surface` makes the word a
    /// pure function of a surface record's `translucency` and `color_value`, and
    /// `client_portal.dat` ships **153** untextured surface records producing **146**
    /// distinct words. This set cannot exceed that, whatever a player does.
    // ORDER-OK: a census, only ever counted; sorted so a report reads the same twice.
    pub(crate) solid_colour_words: std::collections::BTreeSet<u32>,
    /// Groups that shared a texture key with a group that had expanded it under the **other**
    /// `BASE1_CLIPMAP` setting. The clip-map flag is not part of the original's key either, so this
    /// is retail's own behaviour and not a defect — but it decides pixels by resolve order,
    /// so it is counted rather than left invisible.
    pub(crate) clipmap_key_conflicts: u32,
    /// A sample of *which* keys conflicted, so one can be looked at rather than only
    /// counted. Capped at [`CLIPMAP_CONFLICT_SAMPLE`] entries; `clipmap_key_conflicts` is the
    /// full count and this is a sample of it, deliberately bounded because an unbounded list
    /// on a cache is the shape of a leak.
    ///
    /// A conflicting group is best judged by looking at it under both
    /// settings. A count cannot be looked at; a `(texture id, the two settings)` triple
    /// can, by decoding the texture both ways and comparing texel for texel — which answers
    /// the same question and is a sharper instrument than a
    /// screenshot, because it says *how many* texels the bit moves rather than whether a human
    /// noticed.
    pub(crate) clipmap_conflicts: Vec<ClipMapConflict>,
    /// Distinct surface groups resolved, and how many carried a `curr_alpha`
    /// other than `0xFF`. See [`SceneStats::surfaces_resolved`].
    pub(crate) surfaces_resolved: u32,
    pub(crate) surfaces_translucent: u32,
    /// [`SceneConfig::surface_translucency`]. It lives on the cache because
    /// [`build_meshes`] is a free function that every mesh path already hands one.
    pub(crate) surface_translucency: bool,
}

/// How many [`ClipMapConflict`]s a [`BakeCache`] keeps. The whole corpus produces at most five
/// per recording, so this is headroom rather than a limit — but it is a **cap**, because an
/// unbounded list on a long-lived cache is a leak however small its members.
pub(crate) const CLIPMAP_CONFLICT_SAMPLE: usize = 16;

/// One group that arrived on a texture key another group had already expanded under the other
/// `BASE1_CLIPMAP` setting.
///
/// This is retail's behaviour and not a defect: the combined-texture cache key is
/// `(palette DID, indexed-texture DID)` and **nothing else**. Palette restoration passes
/// `(type & BASE1_CLIPMAP)` as a separate argument, so it never reaches
/// the key. So two surface records that share a pair and disagree about the bit share one texture
/// in the original too, and whichever asked first decides how it was expanded. Recorded so
/// that the *size* of that effect is a measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipMapConflict {
    /// The id whose chain the texture was resolved from — the `SurfaceTexture` an `ObjDesc`
    /// substituted, or the surface record's own.
    pub texture: Option<DataId>,
    /// The surface record the second group named, i.e. the one that carried the losing setting.
    pub surface: Option<DataId>,
    /// How the group that got there first expanded it, and how the second one would have.
    pub first_clip_map: bool,
    pub second_clip_map: bool,
}

/// What an `ObjDesc` does to one surface record, and the only thing that can make two objects
/// sharing a setup record need two textures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceAppearance {
    /// The texture-map replacement that supersedes the surface's own `orig_texture_id`.
    /// `None` leaves it alone.
    pub(crate) texture: Option<DataId>,
    /// The shift palette, as the recipe that builds it. `None` leaves the `RenderSurface`'s own
    /// `default_palette_id` in place.
    pub(crate) palette: Option<PaletteComposition>,
}

/// A shift palette as modified-palette construction plus the build:
/// a private copy of `base`, with each range copied in from another palette at the **same
/// absolute indices**.
///
/// The client keys its combined-texture cache on `(palette DataID, indexed-texture DataID)` and
/// a modified palette has no DataID, so every dyed item gets an *uncached* texture
/// Keying on the recipe instead is the same
/// pixels — the recipe determines the table entry for entry — and lets two players in the same
/// outfit share one upload, which a finite descriptor heap needs and D3D9 did not.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PaletteComposition {
    pub(crate) base: DataId,
    /// `(sub-palette id, offset, colour count)` in the wire's **8-entry units**; a colour count of 0 is 256.
    pub(crate) ranges: Vec<(DataId, u32, u32)>,
    /// Range by range, whether the colours are read from the other era's files
    /// ([`TextureStore::look_palette`]): a part the world draws with the look's colours
    /// ([`dereth_client_runtime::models::colours_for_look`]). Empty reads every range here.
    pub(crate) from_look: Vec<bool>,
}

/// What [`LandContext::generate`] makes for one block: the mesh, each cell's composite, the
/// merge key each cell holds a reference on (empty with no composites), every cell's merge
/// key, and each cell's splat layers (empty unless splatting).
type GeneratedBlock = (
    LandblockMesh,
    Vec<Option<TextureSlot>>,
    Vec<MergeKey>,
    Vec<MergeKey>,
    Vec<Option<Arc<TerrainSplat>>>,
);

/// A [`TerrainTextureSource`] over the retail dats, memoising decodes.
///
/// `fill_temp_tex_buffer` asks for the same four or five terrain tiles for every one of the
/// few hundred distinct merge keys, so without the memo the DXT expansion dominates load time.
/// The memo is [`LandContext::sources`], which outlives the block being generated.
struct DatTerrainTextures<'a> {
    textures: TextureStore<'a>,
    memo: RefCell<&'a mut HashMap<DataId, Option<Arc<Bgra8>>>>,
}

/// A [`dereth_primitives::RenderBackend`] that only ever uploads textures.
///
/// [`TerrainMergeCache::get_or_build`] is typed against the backend-neutral
/// [`dereth_primitives::RenderBackend`] trait, so this is the adapter from that trait onto the real
/// device. Meshes and draws never come through it — the terrain
/// path calls `triangle_vertices` and submits the bytes itself — so those two arms are
/// unreachable and say so.
///
/// Uploads use an independent one-shot command list, including runtime mip generation, and
/// preserve an already-open frame. The merged image remains owned by TerrainMergeCache;
/// the adapter does not retain a second CPU image or a second GPU cache link.
struct TextureUploader<'a> {
    gpu: &'a mut Gpu,
    error: Option<RenderError>,
}

/// The handle a failed upload returns. `bind_texture` is simply not called for it.
const NO_TEXTURE: u32 = u32::MAX;

/// The minimum ambient level applied to the sky lighting result before landscape lighting is
/// updated. The client initializes this floor to `0.2`.
const MIN_AMBIENT: f32 = 0.2;

/// The scene as a pair: the world state the simulation owns, and the drawing
/// half that draws it. `WorldScene` reads as its world state (`scene.character`,
/// `scene.camera`, every [`WorldState`] method), and every public method of
/// [`SceneDraw`] is reachable on it with the world handed across, so a caller that holds the
/// whole scene is unchanged by the split.
#[derive(Debug)]
pub struct WorldScene {
    pub world: WorldState,
    pub draw: SceneDraw,
}

impl std::ops::Deref for WorldScene {
    type Target = WorldState;
    fn deref(&self) -> &WorldState {
        &self.world
    }
}

impl std::ops::DerefMut for WorldScene {
    fn deref_mut(&mut self) -> &mut WorldState {
        &mut self.world
    }
}

impl WorldScene {
    /// Load the scene: both halves, as [`SceneDraw::load`] builds them.
    ///
    /// # Errors
    /// As [`SceneDraw::load`].
    pub fn load(
        store: &RetailDatStore,
        gpu: &mut Gpu,
        cfg: SceneConfig,
    ) -> Result<Self, WorldError> {
        let (draw, world) = SceneDraw::load(store, gpu, cfg)?;
        Ok(Self { world, draw })
    }

    /// The whole scene as the shared view an application's world state and a renderer's
    /// drawing half make, so one reader serves both.
    #[must_use]
    pub fn view(&self) -> WorldSceneRef<'_> {
        WorldSceneRef {
            world: &self.world,
            draw: &self.draw,
        }
    }

    /// [`SceneDraw::deferred_surfaces`] on this scene.
    #[must_use]
    pub fn deferred_surfaces(&self) -> u64 {
        self.draw.deferred_surfaces()
    }

    /// [`SceneDraw::last_terrain_surface_built`] on this scene.
    pub const fn last_terrain_surface_built(&self) -> Option<(u32, u32, u32)> {
        self.draw.last_terrain_surface_built()
    }

    /// [`SceneDraw::last_object_texture_built`] on this scene.
    pub const fn last_object_texture_built(&self) -> Option<(u32, u32, u32)> {
        self.draw.last_object_texture_built()
    }

    /// [`SceneDraw::object_texture_census`] on this scene.
    pub const fn object_texture_census(&self) -> (u64, u64, u64, u64) {
        self.draw.object_texture_census()
    }

    /// [`SceneDraw::render_shadow`] on this scene.
    pub const fn render_shadow(&self) -> dereth_client_runtime::render_prefs::RenderPreferences {
        self.draw.render_shadow()
    }

    /// [`SceneDraw::terrain_composites`] on this scene.
    #[must_use]
    pub fn terrain_composites(&self) -> Vec<(u32, TextureSlot)> {
        self.draw.terrain_composites()
    }

    /// [`SceneDraw::terrain_splat_cells`] on this scene.
    #[must_use]
    pub fn terrain_splat_cells(&self) -> (usize, usize) {
        self.draw.terrain_splat_cells()
    }

    /// [`SceneDraw::terrain_splat`] on this scene.
    #[must_use]
    pub fn terrain_splat(&self) -> bool {
        self.draw.terrain_splat()
    }

    /// [`SceneDraw::toggle_terrain_splat`] on this scene.
    pub fn toggle_terrain_splat(&mut self) -> Option<bool> {
        self.draw.toggle_terrain_splat()
    }
}

/// What makes two objects need two sets of meshes: the setup record and everything the server's
/// `ObjDesc` changes about it.
///
/// Applying an object description starts by wiping the prior one, so an object's description *is* its
/// whole appearance and nothing accumulates — which is what makes this a key rather than a
/// running state. The wire's own values are kept verbatim (offsets and lengths in 8-entry
/// units, ids unpacked to full `DataID`s) so that two identical descriptions compare equal
/// whatever order the lists arrived in for one object and not the other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AppearanceKey {
    setup: DataId,
    palette: u32,
    subpalettes: Vec<(u32, u8, u8)>,
    textures: Vec<(u8, u32, u32)>,
    parts: Vec<(u8, u32)>,
}

// `wire_to_params`, `move_to_target`, `stick_target`,
// `move_to_request`, `apply_player_movement`, `apply_movement`, `interpreted_state` and
// `origin_position` live in [`dereth_client_runtime::movement`], and their call sites in object dispatch are
// the world's ([`world_objects`]).

/// Collects one block's object instances by material and building-shell draw owner.
///
/// This is per **block** rather than per scene, and its decode memos live
/// in [`BakeCache`] so that they outlive one block.
struct ObjectBaker<'a> {
    store: &'a RetailDatStore,
    cache: &'a mut BakeCache,
    /// [`SceneConfig::part_degrades`].
    part_degrades: bool,
    /// [`SceneConfig::degrade_levels`].
    degrade_levels: bool,
    /// [`SceneConfig::static_billboards`]. Clear bakes every level in world space.
    billboards: bool,
    // ORDER-OK: keyed by the group key and drained through `order`, which is first-appearance
    // order, so nothing observable depends on the map's iteration.
    groups: HashMap<BatchKey, (Vec<u8>, Vec<LevelChunk>)>,
    order: Vec<BatchKey>,
    /// The degrading parts this bake placed, in placement order.
    placements: Vec<DegradePlacement>,
    /// Where compressed textures' mip chains come from; `None` builds them in the upload. See
    /// [`crate::mip_worker`].
    defer: Option<&'a mut crate::mip_worker::MipWorker>,
    /// Whether `store` and `cache` are another era's look ([`StaticBatch::from_look`]).
    from_look: bool,
    /// How many objects this bake has placed; the current one's index is
    /// [`BatchKey::instance`] for its blending surfaces.
    instances: u32,
    /// A cell's bake keeps its parts one by one ([`CellObjects::parts`]); `None` for a
    /// landblock's, whose objects are drawn whole.
    parts: Option<Vec<CellStaticPart>>,
    /// The index among the cell's statics of the object being placed.
    object: u32,
}

/// One entry of the decoded-surface memo: the surface record, the two polygon flags it
/// reads, and what the object's `ObjDesc` does to it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct GroupKey {
    pub(crate) surface: Option<DataId>,
    pub(crate) two_sided: bool,
    pub(crate) tiled: bool,
    pub(crate) appearance: SurfaceAppearance,
}

/// Draw eligibility belongs to a placement, not SetSurface's material/texture memo.
/// An ordinary object must not merge with a building shell using the same solid material.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BatchKey {
    material: GroupKey,
    building_pass: bool,
    /// The placed object a **blending** surface's triangles came from, `None` for every other
    /// surface. The client queues each object's blended subsets on the alpha list where its
    /// cell draws it, so they come out in the traversal's far-to-near order among every other
    /// translucent thing; a batch merging a whole block's objects has no one distance to take
    /// that place by. Opaque and alpha-tested surfaces write depth and stay merged.
    instance: Option<u32>,
}

impl<'a> ObjectBaker<'a> {
    fn new(
        store: &'a RetailDatStore,
        cache: &'a mut BakeCache,
        part_degrades: bool,
        degrade_levels: bool,
        billboards: bool,
    ) -> Self {
        Self {
            store,
            cache,
            part_degrades,
            degrade_levels,
            billboards,
            groups: HashMap::new(),
            order: Vec::new(),
            placements: Vec::new(),
            defer: None,
            from_look: false,
            instances: 0,
            parts: None,
            object: 0,
        }
    }

    /// Place one of a cell's statics, keeping its parts one by one. `object` is its index
    /// among the cell's statics.
    fn add_cell_static(&mut self, obj_id: DataId, frame: &Frame, object: u32) {
        self.parts.get_or_insert_with(Vec::new);
        self.object = object;
        self.add_object_kind(obj_id, frame, 1.0, false);
    }

    /// One physics body at a world frame: compose each part's frame, scale its mesh, and append
    /// its triangles to the right group.
    ///
    /// Scaled frame composition places a part
    /// ([`dereth_world_render::objects::parts::combine_scaled`]); the mesh itself is scaled
    /// separately through `gfxobj_scale`, and conflating the two makes parts drift apart.
    #[cfg(test)]
    fn add_object(&mut self, obj_id: DataId, frame: &Frame, scale: f32) {
        self.add_object_kind(obj_id, frame, scale, false);
    }

    /// Place one object at a world frame with the building-part flag, returning the degrade
    /// placement of the object's **first** part when that part has one: a building's shell,
    /// whose level the building's openings follow. The index is this baker's own, before
    /// [`Self::finish`].
    fn add_object_kind(
        &mut self,
        obj_id: DataId,
        frame: &Frame,
        scale: f32,
        building_pass: bool,
    ) -> Option<u32> {
        self.instances += 1;
        let mut first_placement = None;
        let store = self.store;
        let parts = self
            .cache
            .parts
            .entry(obj_id)
            .or_insert_with(|| resolve_parts(store, obj_id))
            .clone();
        let s = Vec3::new(scale, scale, scale);
        for (index, part) in parts.into_iter().enumerate() {
            // The part draw guard `if (gfxobj[deg_level] != NULL)`.
            // A baked mesh stands for the near band, and at `d = 0` eleven of the retail
            // dat's degrade records select their `FLT_MAX` terminator, whose `gfxobj_id` is 0:
            // draw nothing. See [`dereth_client_runtime::models::draws_at_near_band`].
            if self.part_degrades && !self.cache.draws_at_near_band(store, part.gfxobj) {
                self.cache.parts_not_drawn += 1;
                continue;
            }
            let world = dereth_world_render::objects::parts::combine_scaled(frame, &part.frame, s);
            // The part's mesh scale, `gfxobj_scale`: the setup's default scale for the part
            // times the object's. The offset above takes the object's scale alone.
            let mesh_scale = Vec3::new(
                part.scale.x * scale,
                part.scale.y * scale,
                part.scale.z * scale,
            );
            // Part initialization fills `gfxobj[i]` from
            // `degrades[i].gfxobj_id`, **not** from the part's own id, and
            // the viewer-distance update re-picks `i` every frame. So a part with a
            // record contributes one run of vertices per level, tagged with the level, and
            // the per-frame pick is [`WorldScene::refresh_degrade_levels`]'s.
            let record = if self.part_degrades && self.degrade_levels {
                self.cache.degrade_record(store, part.gfxobj)
            } else {
                None
            };
            // A cell's part, kept for the per-object test: each level's drawing sphere, which
            // is what the cone test reads at the level the part draws.
            let cell_part = if self.parts.is_some() {
                let key = (part.gfxobj, record.is_some());
                let spheres = if let Some(s) = self.cache.level_spheres.get(&key) {
                    Arc::clone(s)
                } else {
                    let s: LevelSpheres = match &record {
                        Some(info) => info
                            .degrades
                            .iter()
                            .map(|e| {
                                if e.gfxobj_id.0 == 0 {
                                    None
                                } else {
                                    Some(
                                        self.cache
                                            .drawing_sphere(store, e.gfxobj_id)
                                            .unwrap_or(NO_DRAWING_SPHERE),
                                    )
                                }
                            })
                            .collect(),
                        None => Arc::new([Some(
                            self.cache
                                .drawing_sphere(store, part.gfxobj)
                                .unwrap_or(NO_DRAWING_SPHERE),
                        )]),
                    };
                    self.cache.level_spheres.insert(key, Arc::clone(&s));
                    s
                };
                let parts = self.parts.get_or_insert_with(Vec::new);
                // LINT-OK: a part index bounded by the cell's part count. Not a float.
                #[allow(clippy::cast_possible_truncation)]
                let index = parts.len() as u32;
                parts.push(CellStaticPart {
                    object: self.object,
                    setup: obj_id,
                    placement: None,
                    frame: world,
                    scale: dereth_world_render::cells::cull::object_scale(mesh_scale),
                    spheres,
                    drawn: std::cell::Cell::new((0, 0, 0)),
                });
                index
            } else {
                NO_PART
            };
            let Some(info) = record else {
                self.append_mesh(
                    part.gfxobj,
                    &world,
                    mesh_scale,
                    (NO_PLACEMENT, 0, cell_part),
                    false,
                    building_pass,
                );
                continue;
            };
            // The viewer distance is measured to the **level-0** mesh's sort centre: the viewer-distance update
            // reads the level-0 graphics object before it asks `get_degrade` anything, so the point does
            // not move when the level does.
            let centre = {
                let sc = self.cache.sort_center(store, info.degrades[0].gfxobj_id);
                let c = Vec3::new(
                    sc.x * mesh_scale.x,
                    sc.y * mesh_scale.y,
                    sc.z * mesh_scale.z,
                );
                world.origin.add(dereth_terrain::math::localtoglobalvec(
                    dereth_terrain::math::l2g(world.rotation),
                    c,
                ))
            };
            // LINT-OK: a placement index bounded by the block's part count. Not a float.
            #[allow(clippy::cast_possible_truncation)]
            let placement = self.placements.len() as u32;
            if index == 0 {
                first_placement = Some(placement);
            }
            if let Some(p) = self
                .parts
                .as_mut()
                .and_then(|p| p.get_mut(cell_part as usize))
            {
                p.placement = Some(placement);
            }
            // `get_degrade` returns a `degrade_mode` beside the level, and
            // draw-frame calculation turns `draw_pos.frame` toward the
            // viewer for modes 2-5. A world-space vertex cannot turn, so a placement whose
            // record names any such mode is baked **part-local** instead and
            // [`assemble_batches`] applies the frame as it copies. Decided per *record*, not
            // per level, because the level changes at runtime and the bake does not.
            let billboards = self.billboards
                && info.degrades.iter().any(|e| {
                    dereth_world_render::objects::degrade::DegradeMode::from_raw(e.degrade_mode)
                        != dereth_world_render::objects::degrade::DegradeMode::None
                });
            self.placements.push(DegradePlacement {
                centre,
                object_origin: if building_pass {
                    None
                } else {
                    Some(frame.origin)
                },
                scale_z: mesh_scale.z,
                info: Arc::clone(&info),
                level: 0,
                billboards,
                frame: world,
                // Mode 1 until the first `select_levels`, which is also what makes the very
                // first assembly reproduce the world-space bake byte for byte.
                draw_pos: world,
                mode: dereth_world_render::objects::degrade::DegradeMode::None,
            });
            for (level, e) in info.degrades.iter().enumerate() {
                // `gfxobj_id == 0` is a level with no graphics object: that level draws
                // nothing, so it contributes no chunk and selecting it draws nothing.
                if e.gfxobj_id.0 == 0 {
                    continue;
                }
                // LINT-OK: a level index; the longest shipped record has six. Not a float.
                #[allow(clippy::cast_possible_truncation)]
                let level = level as u32;
                self.append_mesh(
                    e.gfxobj_id,
                    &world,
                    mesh_scale,
                    (placement, level, cell_part),
                    billboards,
                    building_pass,
                );
            }
        }
        first_placement
    }

    /// One graphics object's triangles into the group buffers, tagged with the placement,
    /// level and cell part they belong to (`owner`). Split out of [`Self::add_object_kind`].
    // The final flag carries building drawing's owner through the existing per-level bake.
    fn append_mesh(
        &mut self,
        gfxobj: DataId,
        world: &Frame,
        scale: Vec3,
        owner: (u32, u32, u32),
        local: bool,
        building_pass: bool,
    ) {
        let (placement, level, part) = owner;
        let store = self.store;
        let groups = self
            .cache
            .geometry
            .entry(gfxobj)
            .or_insert_with(|| build_gfxobj(store, gfxobj))
            .clone();
        for g in &groups {
            let blends = g.surface.is_some_and(|id| {
                *self.cache.surface_blends.entry(id).or_insert_with(|| {
                    read_surface(store, id).is_some_and(|s| {
                        s.surface_type
                            & (dereth_world_render::objects::draw::surface_type::ALPHA
                                | dereth_world_render::objects::draw::surface_type::INV_ALPHA
                                | dereth_world_render::objects::draw::surface_type::ADDITIVE
                                | dereth_world_render::objects::draw::surface_type::TRANSLUCENT)
                            != 0
                    })
                })
            });
            // Scenery, buildings and statics carry no object-description overrides; block
            // initialization makes them straight from the dat.
            let key = BatchKey {
                material: GroupKey {
                    surface: g.surface,
                    two_sided: g.two_sided,
                    tiled: g.tiled,
                    appearance: SurfaceAppearance::default(),
                },
                building_pass,
                instance: blends.then_some(self.instances),
            };
            let (buf, chunks) = self.groups.entry(key.clone()).or_insert_with(|| {
                self.order.push(key);
                (Vec::new(), Vec::new())
            });
            // LINT-OK: a byte offset into one landblock's vertex buffer. Not a float.
            #[allow(clippy::cast_possible_truncation)]
            let start = buf.len() as u32;
            let rot = dereth_terrain::math::l2g(world.rotation);
            for (i, (p, u, v)) in g.vertices.iter().enumerate() {
                let scaled = Vec3::new(p.x * scale.x, p.y * scale.y, p.z * scale.z);
                // A billboarding placement is baked in the part's own frame
                // and transformed at assembly time by `draw_pos`; everything else is baked in
                // world space. With mode 1 the two are the same
                // arithmetic in the same order, so the assembled bytes are identical.
                let w = if local {
                    scaled
                } else {
                    dereth_terrain::math::localtoglobal(world, scaled)
                };
                buf.extend_from_slice(&w.x.to_le_bytes());
                buf.extend_from_slice(&w.y.to_le_bytes());
                buf.extend_from_slice(&w.z.to_le_bytes());
                // The vertex normal, in the same space as the position: the
                // placement's rotation for a world-space bake, the part's own for a
                // billboarding one (which `append_billboarded` rotates at assembly). A
                // uniform `scale` does not touch it -- `D3DRS_NORMALIZENORMALS = 1`. A
                // part's own scale can differ per axis, and then the normal goes through the
                // inverse transpose, `n / scale`, before it is renormalised.
                let n = g.normals.get(i).copied().unwrap_or(Vec3::ZERO);
                let n = baked_normal(n, scale);
                let n = if local {
                    n
                } else {
                    dereth_terrain::math::localtoglobalvec(rot, n)
                };
                buf.extend_from_slice(&n.x.to_le_bytes());
                buf.extend_from_slice(&n.y.to_le_bytes());
                buf.extend_from_slice(&n.z.to_le_bytes());
                // White: with the diffuse colour source set to the vertex, this is the material the
                // fixed-function lighting multiplies the lights into. The alpha
                // byte written here is a placeholder: `finish` stamps surface setup's
                // current alpha over it once the group's surface has been resolved
                // ([`stamp_vertex_alpha`]).
                buf.extend_from_slice(&vertex_diffuse(0xFF).to_le_bytes());
                buf.extend_from_slice(&u.to_le_bytes());
                buf.extend_from_slice(&v.to_le_bytes());
            }
            // LINT-OK: as above. Not a float conversion.
            #[allow(clippy::cast_possible_truncation)]
            let end = buf.len() as u32;
            if end != start {
                chunks.push(LevelChunk {
                    placement,
                    level,
                    start,
                    end,
                    part,
                });
            }
        }
    }

    /// Resolve every group's surface to a pipeline state and a texture, and split the result
    /// into the opaque batches and the blending ones.
    fn finish(mut self, gpu: &mut Gpu) -> Result<BakedGroups, RenderError> {
        let textures = TextureStore::with_environment_texture_detail(
            self.store,
            self.cache.environment_texture_detail,
        );
        let mut opaque = Vec::new();
        let mut blended = Vec::new();
        for key in std::mem::take(&mut self.order) {
            let Some((mut vertices, chunks)) = self.groups.remove(&key) else {
                continue;
            };
            if vertices.is_empty() {
                continue;
            }
            // A batch no degrade record governs keeps its chunk list empty and is
            // drawn whole, which is what makes the `degrade_levels` control byte-exact.
            let degrades = chunks.iter().any(|c| c.placement != NO_PLACEMENT);
            // A cell's batch says which of its bytes are which part's. A degrading batch's
            // drawn buffer is assembled, and so are its runs ([`assemble_batches`]); the rest
            // are drawn as baked, and the runs are the chunks'.
            let part_runs = if degrades {
                Vec::new()
            } else {
                part_runs_of(chunks.iter().map(|c| (c.part, c.start, c.end)))
            };
            let chunks = if degrades { chunks } else { Vec::new() };
            let honour = self.cache.surface_translucency;
            let resolved = match self.defer.as_deref_mut() {
                Some(w) => match self.cache.resolve_with(
                    self.store,
                    &textures,
                    gpu,
                    key.material,
                    Some(w),
                )? {
                    Some(r) => r,
                    // Its chain is being built: this bake will be taken again.
                    None => continue,
                },
                None => self
                    .cache
                    .resolve(self.store, &textures, gpu, key.material)?,
            };
            // Surface setup returns `curr_alpha` per surface, and `GroupKey` *is*
            // the surface, so one stamp covers every vertex the group collected.
            stamp_vertex_alpha(
                &mut vertices,
                if honour { resolved.vertex_alpha } else { 0xFF },
            );
            let (pkey, alpha_ref, slot) = (resolved.key, resolved.alpha_ref, resolved.texture);
            // The cull mode is whatever surface setup chose -- CW unless `sides_type == 1`.
            // The Z-up -> D3D swap is a reflection and does invert winding, but the client
            // applies the same swap in its own world matrix and still issues `D3DCULL_CW`, and
            // the polygon's vertex order reaches the rasteriser unchanged through
            // `dereth_client_runtime::models::triangulate`. Measured: forcing `Cull::None` and letting
            // `from_surface` decide produce the same frame over Holtburg, so nothing is being
            // culled that should be drawn.
            let sphere = vertex_sphere(&vertices);
            let batch = StaticBatch {
                key: pkey,
                key_detail: resolved.key_detail,
                key_force_alpha: resolved.key_force_alpha,
                surface_type: resolved.surface_type,
                building_pass: key.building_pass,
                alpha_ref,
                texture: slot,
                sampler: resolved.sampler,
                luminosity: resolved.luminosity,
                sphere,
                vertices,
                chunks,
                // Filled by the first `assemble_batches`; a batch with no chunks never uses it.
                active: Vec::new(),
                from_look: self.from_look,
                part_runs,
            };
            if batch.key.alpha_blend {
                blended.push(batch);
            } else {
                opaque.push(batch);
            }
        }
        Ok((opaque, blended, self.placements))
    }

    /// [`Self::finish`], and the cell parts the bake kept.
    fn finish_cell(
        mut self,
        gpu: &mut Gpu,
    ) -> Result<(BakedGroups, Vec<CellStaticPart>), RenderError> {
        let parts = self.parts.take().unwrap_or_default();
        Ok((self.finish(gpu)?, parts))
    }
}

/// The runs of a buffer whose bytes belong to cell parts, from `(part, start, end)` in buffer
/// order, adjacent runs of one part merged. Empty when no run belongs to a cell part.
fn part_runs_of(runs: impl Iterator<Item = (u32, u32, u32)>) -> Vec<PartRun> {
    let mut out: Vec<PartRun> = Vec::new();
    for (part, start, end) in runs {
        if part == NO_PART {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.part == part && last.end == start => last.end = end,
            _ => out.push(PartRun { part, start, end }),
        }
    }
    out
}

/// What one [`ObjectBaker`] produced: the opaque batches, the blending ones, and the
/// degrading placements that index into both.
type BakedGroups = (Vec<StaticBatch>, Vec<StaticBatch>, Vec<DegradePlacement>);

/// Which object an [`EmitterDegrade`] row belongs to.
///
/// ORDER-OK: `Ord` is derived so a test can group rows by owner in a `BTreeSet`; the order is
/// this enum's declaration order and nothing observable depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EmitterOwner {
    /// A baked placement with a, by resident-block and host index.
    Placement(usize, usize),
    /// One of the server's objects.
    Object(ObjectId),
    /// The local body.
    Body,
}

/// One emitter's two cut-off distances and its current viewer distance. See
/// [`WorldScene::emitter_degrade_probe`] and [`manager_degrade_distance`].
#[derive(Debug, Clone, Copy)]
pub struct EmitterDegrade {
    pub owner: EmitterOwner,
    /// The owning object's origin in the renderer's space, which is what the viewer distance is measured
    /// from. Carried so a test can place the camera at a chosen distance from *this* object
    /// rather than from whichever host happens to be first.
    pub origin: Vec3,
    /// `ParticleManager`'s key.
    pub emitter: u32,
    /// The graphics-object id whose `GfxObjDegradeInfo` decides both distances.
    pub gfxobj: DataId,
    /// The particle emitter's `degrade_distance` — what the client would ask.
    pub own_distance: f32,
    /// What this build asks instead: [`manager_degrade_distance`] over the whole manager.
    pub manager_distance: f32,
    /// The viewer distance, measured
    /// from the object's origin to the camera.
    pub cypt: f32,
    /// Live particles this emitter is holding right now.
    pub live: usize,
}

/// What [`part_level`] chose for one part this frame: viewer-distance updating's two outputs
/// and `get_degrade`'s two.
#[derive(Debug, Clone, Copy)]
struct PartFrameChoice {
    /// The part's viewer distance -- to its own sort centre, or the object's origin when the
    /// object shares -- and therefore its depth-sort key.
    cypt: f32,
    /// The viewer heading the draw frame was turned toward, from the same measurement.
    heading: Vec3,
    /// What `get_degrade` was handed: the viewer distance over the z scale.
    distance: f32,
    level: u32,
    mode: dereth_world_render::objects::degrade::DegradeMode,
    /// `calc_draw_frame(pos, mode, viewer_heading)`.
    draw_pos: Frame,
}

/// Everything one object pass accumulates: five out-parameters of [`draw_part`] would make a
/// nine-argument
/// function and a transposition waiting to happen.
#[derive(Debug, Default)]
struct PartPass {
    /// The clip and alpha mesh lists.
    lists: dereth_world_render::objects::alpha::AlphaLists,
    /// One entry per `AlphaLists::push`, in **push** order. An `AlphaEntry`'s `mesh` handle is
    /// its index here, which is what lets `FlushAlphaList` get back to the part and the subset
    /// after `take_draw_order` has reordered the entries.
    queued: Vec<QueuedSubset>,
    /// One entry per subset put on the device, in **device submission** order.
    trace: Vec<PartSubsetDraw>,
    counts: PartDrawCounts,
    /// One `(prepared particle, subset)` per particle entry pushed onto `lists`; an entry's
    /// `mesh` handle is [`PARTICLE_ENTRY`] or'd with its index here.
    particle_queued: Vec<(usize, usize)>,
    /// The particle entries on each list, kept out of the parts' census.
    particle_clip: usize,
    particle_blend: usize,
    /// The blended static batches on the alpha list, kept out of the parts' census.
    static_blend: usize,
}

/// The bit that marks an alpha-list entry's `mesh` handle as a particle's (indexing
/// `PartPass::particle_queued`) rather than a part's (indexing `PartPass::queued`).
const PARTICLE_ENTRY: u32 = 0x8000_0000;
/// The bit that marks an alpha-list entry's `mesh` handle as a blended static batch's
/// (indexing the pass's sorted [`StaticBlendRef`]s).
const STATIC_ENTRY: u32 = 0x4000_0000;

/// Where a queued blended static batch lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticBlendSource {
    /// `BlockDraw::blended` of this landblock.
    Block((i32, i32)),
    /// `statics_blended` of this environment cell.
    Cell(u32),
}

/// One blended static batch waiting for the object pass's alpha list.
///
/// The client queues a static's blended subsets on the alpha list where its cell draws it,
/// among every creature's and particle's, and flushes the list in that order. This build
/// draws the moving parts in one far-to-near pass after the walk, so a static takes its place
/// among them by the same measure: its viewer distance.
#[derive(Debug, Clone, Copy)]
struct StaticBlendRef {
    source: StaticBlendSource,
    /// The index in the source's batch list.
    batch: usize,
    /// The distance from the viewer to the batch's sphere centre.
    cypt: f32,
    /// For a cell's batch, `(frame stamp, cell id, turn)`: only the parts that turn of that
    /// cell's object draw took on that stamp are drawn ([`CellStaticPart::drawn`]).
    drawn_by: Option<(u32, u32, u32)>,
    /// Whether the batch was queued by a land cell's object draw, under the sun's light set,
    /// rather than an interior cell's.
    sun: bool,
}

/// One deferred subset, as [`PartPass::queued`] holds it.
#[derive(Debug, Clone, Copy)]
struct QueuedSubset {
    /// Index into the frame's `PartSubmission` list.
    submission: u32,
    /// Index into that submission's `meshes`.
    subset: usize,
    probe: PartSubsetDraw,
}

/// [`draw_part`]'s three running totals.
#[derive(Debug, Clone, Copy, Default)]
struct PartDrawCounts {
    /// Animated parts submitted -- the material-alpha denominator.
    parts: u32,
    /// Of those, how many were drawn through a cloned material's alpha.
    material: u32,
    /// Subsets the subset draw drew in place rather than deferring.
    immediate: usize,
}

/// One part of one moving object, queued for submission this frame. It carries the sort
/// key, and the alpha lists index into it.
///
/// It exists because the client's object pass has a phase between *deciding what draws* and
/// *drawing it* -- the first block-draw pass sorts the cell's shadow parts before the second draws
/// them -- and this is where that phase keeps its parts.
#[derive(Debug, Clone, Copy)]
struct PartSubmission<'a> {
    /// The server object this part belongs to, or `None` for the **local body** or a placed
    /// static.
    object: Option<ObjectId>,
    /// Whether the part is a placed static's (an animated piece of scenery or furniture): it
    /// has no object id, so it is neither the local body nor anything the selection ray can
    /// take.
    placed: bool,
    /// Its index in the object's part array.
    index: usize,
    part: &'a dereth_animation::parts::PhysicsPart,
    /// The part's draw-position frame.
    draw_pos: Frame,
    /// The part's viewer distance, its depth-sort key.
    cypt: f32,
    /// `gfxobj[deg_level]`'s subsets, one [`PartMesh`] each. An `AlphaEntry`'s `surface_num`
    /// indexes this and its `mesh` handle indexes the submission list.
    meshes: &'a [PartMesh],
    /// The selected level's drawing sphere, in the part's own unscaled space — what
    /// mesh drawing hands to the view-cone test. See `WorldScene::part_cone`.
    drawing_sphere: Option<(Vec3, f32)>,
    /// Whether the owning object stands in an outdoor cell --
    /// landscape rendering draws it with sunlight use 1 and cell drawing draws the rest with
    /// sunlight use 0, which makes mesh drawing minimize object lighting.
    outdoors: bool,
    /// The render stage, kept separate from `outdoors` because building interiors are drawn
    /// before the clear with env-cell lighting.
    before_depth_clear: bool,
    /// The cell whose object list drew this part, as
    /// `WorldScene::object_draw_cell` resolves it, or, for a part in a building's room that an
    /// outdoor viewer's landscape walk draws through its registration outside, that land cell.
    ///
    /// Cell drawing installs *that* cell's own `portal_view` before drawing its objects, so the
    /// cone this part is tested against is the cell's
    /// and not the screen's. `None` is the local
    /// body in a scene with no cell, or an object whose cell the walk did not name — both fall
    /// back to the full-screen cone, which is the unclipped superset.
    cell: Option<CellId>,
}

/// The object and cell vertex is FVF `0x152`: a float3 position, float3
/// normal, diffuse color, and float2 UV, totaling 36 bytes, which is what
/// the mesh-material builder creates for a graphics object and what the
/// fixed-function lighting reads the normal out of. The terrain's 24-byte `XyzDiffuseTex1`
/// layout has no normal, so an object emitted in it could not be lit.
/// The terrain keeps [`LAND_VERTEX_STRIDE`]: land polygon drawing turns fixed-function lighting off and
/// its colours come from baked landscape lighting.
pub(crate) const OBJECT_VERTEX_STRIDE: usize = 36;
/// The byte offset of the `D3DFVF_DIFFUSE` word inside an object vertex: after the position
/// and the normal.
pub(crate) const OBJECT_DIFFUSE_OFFSET: usize = 24;
/// The byte offset of the alpha channel inside an object vertex: the diffuse word's high byte.
pub(crate) const VERTEX_ALPHA_BYTE: usize = OBJECT_DIFFUSE_OFFSET + 3;

/// [`resolve_surface`]'s answer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedSurface {
    pub(crate) key: PipelineKey,
    /// Authored; draw eligibility cannot be inferred from a bound solid texel.
    pub(crate) surface_type: u32,
    /// [`Self::key`] re-run with `SurfaceContext::material_has_alpha = Some(true)`.
    ///
    /// The material-state table's row 13: a non-null material whose
    /// `has_alpha` is set forces `SRCALPHA / INVSRCALPHA`, drops the alpha test and turns the
    /// depth write off — unless the surface was *already* blending without a test, in which
    /// case it is left alone, which is what spares an additive glow its `ONE / ONE`.
    pub(crate) key_material_alpha: PipelineKey,
    /// [`Self::key`] re-run with `SurfaceContext::detail_in_stage1` set, which
    /// is the subset draw's third surface setup argument: the same pipeline state
    /// with `StageOps::SINGLE_PASS_DETAIL` in place of `StageOps::BASE`. The draw picks,
    /// because whether a detail surface is installed is a property of the *frame* and not of
    /// the surface.
    pub(crate) key_detail: PipelineKey,
    /// [`Self::key`] re-run with `SurfaceContext::force_alpha` set: the state the alpha-list
    /// flush gives an entry queued by "Multiple Pass Alpha". See [`PartMesh::key_force_alpha`].
    pub(crate) key_force_alpha: PipelineKey,
    pub(crate) alpha_ref: u8,
    /// The current alpha, the material setup's **return value**: `0xFF` for
    /// everything except a `TRANSLUCENT` surface, where it is
    /// `(int)((1 - surface.translucency) * 255)`.
    ///
    /// The caller writes it into the alpha byte of every vertex's `D3DFVF_DIFFUSE` word —
    /// the polygon renderer writes `alpha << 0x18 | 0xffffff` into it literally, and the vertex
    /// copy bakes the
    /// same `x << 0x18 | 0xffffff` into a constructed mesh's vertices. Dropping it submits a
    /// translucent surface at alpha 255 through its own `SRCALPHA/INVSRCALPHA` blend, i.e.
    /// fully opaque.
    pub(crate) vertex_alpha: u8,
    /// The surface's luminosity, for the subset draw's emissive.
    pub(crate) luminosity: f32,
    pub(crate) texture: Option<TextureSlot>,
    /// The key
    /// [`Self::texture`] was uploaded under, so that [`BakeCache`] can tell a fresh upload
    /// from an `AddRef` on a texture it already owns. [`dereth_render::descriptor::UNCACHED`]
    /// when there is no texture, or when the original would not have cached it either.
    /// A [`dereth_render::TextureKey`] rather than a bare `u64`: three
    /// producers put unrestricted 32-bit words in the same halves of the same key, and one of
    /// the three is this type's own solid-colour arm.
    pub(crate) texture_key: dereth_render::TextureKey,
    /// The surface record's `BASE1_CLIPMAP` bit as this resolve expanded the
    /// texture with. It is **not** part of [`Self::texture_key`], because it is not part of
    /// the original's either; it is carried so that two groups sharing a key and disagreeing
    /// about it can be counted.
    pub(crate) clip_map: bool,
    /// Whether [`Self::texture`] is surface setup's **solid-colour texel**
    /// rather than a texture out of the dat.
    ///
    /// The two are keyed in the same space and cached by the same table, and they are two
    /// different transcriptions: the textured half is the combined-texture cache;
    /// the untextured half is this build's stand-in for one device-wide solid-color
    /// texture. A single "cache hits" counter over both **cannot fail** when one of them is
    /// unwired, because the other keeps it non-zero — with the whole textured key
    /// forced to `UNCACHED`, a streaming test stays green off the
    /// solid-colour hits alone.
    pub(crate) solid_texel: bool,
    pub(crate) sampler: u32,
    /// The mesh's per-subset stippled-or-alpha mask. See
    /// [`PartMesh::subset_mask`]. `0` for a surface that is not in the dat, which is the same
    /// answer `RenderState::default()`'s `type == 0` gives the rest of this function.
    pub(crate) subset_mask: u32,
    /// Sub-palette ranges this surface's shift palette could not apply.
    palette_failures: u32,
    /// Its base palette was not in the dat.
    palette_missing: bool,
}

/// The sixteen floats `dereth_render`'s legacy shader set actually wants.
///
/// **This transposes, and that is a shader-layout fact being handled rather than a convention.**
/// `legacy.hlsl` transforms with `mul(vector, matrix)`, which is HLSL's row-vector form, and
/// `D3DCompile` is invoked without `/Zpr`, so a `float4x4` in a constant buffer is packed
/// **column-major**: element `[i][j]` is read from `data[j*4 + i]`. `glam::Mat4::to_cols_array`
/// — which `PerFrameConstants::from_view` and `PerDrawConstants::identity` both use — writes
/// `data[j*4 + i] = M[i][j]`, so the shader evaluates `Mᵀ·v` where the caller meant `M·v`.
/// Identity is its own transpose, which is why a full-screen quad (identity matrices, and
/// a pre-transformed vertex format that skips them anyway) never showed it.
///
/// `dereth_render::hlsl_matrix` does it at the source, so this is
/// a re-export. The tests below evaluate the shader's own indexing arithmetic, and
/// they are what would catch the layout regressing again.
pub use dereth_render::device::hlsl_matrix;

#[cfg(test)]
#[path = "../world_texture_minification_tests.rs"]
mod texture_minification;

#[cfg(test)]
#[path = "../world_building_shell_visibility_tests.rs"]
mod building_shell_visibility;

#[cfg(test)]
#[path = "../world_building_openings_tests.rs"]
mod building_openings;

#[cfg(test)]
#[path = "../world_part_scale_tests.rs"]
mod part_scale;

#[cfg(test)]
#[path = "../world_animated_scenery_tests.rs"]
mod animated_scenery;

#[cfg(test)]
#[path = "../world_env_surface_visibility_tests.rs"]
mod env_surface_visibility;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// The ten counters `App`'s three census lines print.
///
/// The projection is here, beside the counters it reads, so that a field added to
/// [`SceneStats`] that the lines are meant to print has one place to be added and a station
/// (`app.rs`'s two census tests) that reads this conversion rather than the whole struct.
impl From<&SceneStats> for dereth_client_runtime::present::SceneCensus {
    fn from(s: &SceneStats) -> Self {
        Self {
            blocks_meshed: s.blocks_meshed,
            terrain_surfaces: s.terrain_surfaces,
            scenery_objects: s.scenery_objects,
            buildings: s.buildings,
            static_objects: s.static_objects,
            object_triangles: s.object_triangles,
            server_objects_animated: s.server_objects_animated,
            server_objects_held: s.server_objects_held,
            server_object_setups: s.server_object_setups,
            server_object_triangles: s.server_object_triangles,
        }
    }
}

/// A shared view of the two halves of the scene: the world state the application
/// owns and the drawing half the presentation owns. It reads as the world state (Deref), and
/// it is a [`dereth_client_runtime::present::Scene`], so a caller that only reads the drawn world sees it
/// exactly as it saw a whole `WorldScene`.
#[derive(Debug, Clone, Copy)]
pub struct WorldSceneRef<'a> {
    pub world: &'a WorldState,
    pub draw: &'a SceneDraw,
}

/// The same view, holding both halves mutably: a [`dereth_client_runtime::present::SceneMut`].
#[derive(Debug)]
pub struct WorldSceneMut<'a> {
    pub world: &'a mut WorldState,
    pub draw: &'a mut SceneDraw,
}

impl std::ops::Deref for WorldSceneRef<'_> {
    type Target = WorldState;
    fn deref(&self) -> &WorldState {
        self.world
    }
}

impl std::ops::Deref for WorldSceneMut<'_> {
    type Target = WorldState;
    fn deref(&self) -> &WorldState {
        self.world
    }
}

impl std::ops::DerefMut for WorldSceneMut<'_> {
    fn deref_mut(&mut self) -> &mut WorldState {
        self.world
    }
}

// The private supertrait module seals these operations to the scene views;
// callers cannot implement the traits for unrelated state.
mod sealed {
    use super::{SceneDraw, WorldState};
    pub trait SceneHalves {
        fn halves(&self) -> (&WorldState, &SceneDraw);
    }
    pub trait SceneHalvesMut: SceneHalves {
        fn halves_mut(&mut self) -> (&mut WorldState, &mut SceneDraw);
    }
}
pub(crate) use sealed::{SceneHalves, SceneHalvesMut};

/// Read operations shared by an owned scene and its borrowed views.
pub trait SceneReads: sealed::SceneHalves {
    /// [`SceneDraw::world_fog_state`] on the scene.
    fn world_fog_state(&self) -> dereth_render::camera::FogParams {
        let (world, draw) = self.halves();
        draw.world_fog_state(world)
    }

    /// [`SceneDraw::world_fog`] on the scene.
    fn world_fog(&self) -> dereth_render::camera::FogParams {
        let (_, draw) = self.halves();
        draw.world_fog()
    }

    /// [`SceneDraw::detail_texturing`] on the scene.
    fn detail_texturing(&self) -> dereth_world_render::detail::DetailTexturing {
        let (_, draw) = self.halves();
        draw.detail_texturing()
    }

    /// [`SceneDraw::live_appearances`] on the scene.
    fn live_appearances(&self) -> usize {
        let (_, draw) = self.halves();
        draw.live_appearances()
    }

    /// [`SceneDraw::clipmap_conflicts`] on the scene.
    fn clipmap_conflicts(&self) -> &[ClipMapConflict] {
        let (_, draw) = self.halves();
        draw.clipmap_conflicts()
    }

    /// [`SceneDraw::appearance_link_counts`] on the scene.
    fn appearance_link_counts(&self) -> Vec<usize> {
        let (_, draw) = self.halves();
        draw.appearance_link_counts()
    }

    /// [`SceneDraw::cell_static_batches`] on the scene.
    fn cell_static_batches(&self, cell: CellId) -> usize {
        let (_, draw) = self.halves();
        draw.cell_static_batches(cell)
    }

    /// [`SceneDraw::resident_blocks`] on the scene.
    fn resident_blocks(&self) -> usize {
        let (_, draw) = self.halves();
        draw.resident_blocks()
    }

    /// [`SceneDraw::pending_slot_count`] on the scene.
    fn pending_slot_count(&self) -> usize {
        let (_, draw) = self.halves();
        draw.pending_slot_count()
    }

    /// [`SceneDraw::released_block_count`] on the scene.
    fn released_block_count(&self) -> usize {
        let (_, draw) = self.halves();
        draw.released_block_count()
    }

    /// [`SceneDraw::last_terrain_surface_built`] on the scene.
    fn last_terrain_surface_built(&self) -> Option<(u32, u32, u32)> {
        let (_, draw) = self.halves();
        draw.last_terrain_surface_built()
    }

    /// [`SceneDraw::last_object_texture_built`] on the scene.
    fn last_object_texture_built(&self) -> Option<(u32, u32, u32)> {
        let (_, draw) = self.halves();
        draw.last_object_texture_built()
    }

    /// [`SceneDraw::object_texture_census`] on the scene.
    fn object_texture_census(&self) -> (u64, u64, u64, u64) {
        let (_, draw) = self.halves();
        draw.object_texture_census()
    }

    /// [`SceneDraw::render_shadow`] on the scene.
    fn render_shadow(&self) -> dereth_client_runtime::render_prefs::RenderPreferences {
        let (_, draw) = self.halves();
        draw.render_shadow()
    }

    /// [`SceneDraw::block_bake`] on the scene.
    fn block_bake(&self, block: (i32, i32)) -> Option<BlockBake> {
        let (_, draw) = self.halves();
        draw.block_bake(block)
    }

    /// [`SceneDraw::window_rings`] on the scene.
    fn window_rings(&self) -> Vec<WindowRing> {
        let (world, draw) = self.halves();
        draw.window_rings(world)
    }

    /// [`SceneDraw::alpha_list_batches`] on the scene.
    fn alpha_list_batches(&self) -> usize {
        let (_, draw) = self.halves();
        draw.alpha_list_batches()
    }

    /// [`SceneDraw::part_degrade_probe`] on the scene.
    fn part_degrade_probe(&self) -> Vec<PartLevelProbe> {
        let (world, draw) = self.halves();
        draw.part_degrade_probe(world)
    }

    /// [`SceneDraw::degrade_probe`] on the scene.
    fn degrade_probe(&self) -> Vec<StaticLevelProbe> {
        let (world, draw) = self.halves();
        draw.degrade_probe(world)
    }

    /// [`SceneDraw::force_level`] on the scene.
    fn force_level(&self) -> i32 {
        let (_, draw) = self.halves();
        draw.force_level()
    }

    /// [`SceneDraw::host_pending_events`] on the scene.
    fn host_pending_events(&self) -> usize {
        let (_, draw) = self.halves();
        draw.host_pending_events()
    }

    /// [`SceneDraw::host_bodies`] on the scene.
    fn host_bodies(&self) -> Vec<HostBody> {
        let (world, draw) = self.halves();
        draw.host_bodies(world)
    }

    /// [`SceneDraw::region`] on the scene.
    fn region(&self) -> &dereth_assets::Region {
        let (_, draw) = self.halves();
        draw.region()
    }

    /// [`SceneDraw::character_built_from`] on the scene.
    fn character_built_from(&self) -> &[DataId] {
        let (_, draw) = self.halves();
        draw.character_built_from()
    }

    /// [`SceneDraw::character_part_textures`] on the scene.
    fn character_part_textures(&self) -> Vec<Vec<u32>> {
        let (_, draw) = self.halves();
        draw.character_part_textures()
    }

    /// [`SceneDraw::target_projection`] on the scene.
    fn target_projection(
        &self,
        id: ObjectId,
        viewport: (u32, u32),
    ) -> Option<dereth_client_contract::target::Projection> {
        let (world, draw) = self.halves();
        draw.target_projection(world, id, viewport)
    }

    /// [`SceneDraw::upload_reservation`] on the scene.
    fn upload_reservation(&self) -> usize {
        let (_, draw) = self.halves();
        draw.upload_reservation()
    }

    /// [`SceneDraw::reserve_upload_arena`] on the scene.
    fn reserve_upload_arena(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
        let (_, draw) = self.halves();
        draw.reserve_upload_arena(gpu)
    }

    /// [`SceneDraw::degrade_globals`] on the scene.
    fn degrade_globals(&self) -> dereth_world_render::objects::degrade::DegradeGlobals {
        let (world, draw) = self.halves();
        draw.degrade_globals(world)
    }

    /// [`SceneDraw::weather_enabled`] on the scene.
    fn weather_enabled(&self) -> Option<bool> {
        let (_, draw) = self.halves();
        draw.weather_enabled()
    }

    /// [`SceneDraw::landscape_lighting`] on the scene.
    fn landscape_lighting(&self) -> LandscapeLighting {
        let (_, draw) = self.halves();
        draw.landscape_lighting()
    }

    /// [`SceneDraw::viewer_block_colours`] on the scene.
    #[must_use]
    fn viewer_block_colours(&self) -> Option<Vec<[u8; 3]>> {
        let (world, draw) = self.halves();
        draw.viewer_block_colours(world)
    }

    /// [`SceneDraw::view_params`] on the scene.
    fn view_params(&self, width: u32, height: u32) -> ViewParams {
        let (world, draw) = self.halves();
        draw.view_params(world, width, height)
    }

    /// [`SceneDraw::effective_viewport`] on the scene.
    fn effective_viewport(&self, width: u32, height: u32) -> dereth_render::camera::Viewport {
        let (world, draw) = self.halves();
        draw.effective_viewport(world, width, height)
    }

    /// [`SceneDraw::set_selected_object_id`] on the scene.
    fn set_selected_object_id(&self, id: Option<ObjectId>) {
        let (_, draw) = self.halves();
        draw.set_selected_object_id(id)
    }

    /// [`SceneDraw::take_selected_part_drawn`] on the scene.
    fn take_selected_part_drawn(&self) -> bool {
        let (_, draw) = self.halves();
        draw.take_selected_part_drawn()
    }

    /// [`SceneDraw::sun_light`] on the scene.
    fn sun_light(&self) -> Option<D3dLight> {
        let (_, draw) = self.halves();
        draw.sun_light()
    }

    /// [`SceneDraw::world_ambient_color`] on the scene.
    fn world_ambient_color(&self) -> [f32; 3] {
        let (world, draw) = self.halves();
        draw.world_ambient_color(world)
    }

    /// [`SceneDraw::object_light_set`] on the scene.
    fn object_light_set(&self, centre: Vec3, radius: f32, outdoors: bool) -> Vec<D3dLight> {
        let (_, draw) = self.halves();
        draw.object_light_set(centre, radius, outdoors)
    }

    /// [`SceneDraw::light_pools`] on the scene.
    fn light_pools(&self) -> &LightPools {
        let (_, draw) = self.halves();
        draw.light_pools()
    }

    /// [`SceneDraw::cell_light_objects`] on the scene.
    fn cell_light_objects(&self) -> usize {
        let (_, draw) = self.halves();
        draw.cell_light_objects()
    }

    /// [`SceneDraw::env_cell_burned_vertices`] on the scene.
    fn env_cell_burned_vertices(&self, cell: CellId) -> Vec<(Vec3, Vec3, [u8; 3])> {
        let (_, draw) = self.halves();
        draw.env_cell_burned_vertices(cell)
    }

    /// [`SceneDraw::cell_block_origin`] on the scene.
    fn cell_block_origin(&self, cell: CellId) -> Option<(f32, f32)> {
        let (_, draw) = self.halves();
        draw.cell_block_origin(cell)
    }

    /// [`SceneDraw::draw`] on the scene.
    fn draw(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
        let (world, draw) = self.halves();
        draw.draw(world, gpu)
    }

    /// [`SceneDraw::emitter_degrade_probe`] on the scene.
    fn emitter_degrade_probe(&self) -> Vec<EmitterDegrade> {
        let (world, draw) = self.halves();
        draw.emitter_degrade_probe(world)
    }

    /// [`SceneDraw::emitter_host_origins`] on the scene.
    fn emitter_host_origins(&self) -> Vec<Vec3> {
        let (world, draw) = self.halves();
        draw.emitter_host_origins(world)
    }

    /// [`SceneDraw::drawn_particles`] on the scene.
    fn drawn_particles(&self) -> crate::particles::ParticleStats {
        let (_, draw) = self.halves();
        draw.drawn_particles()
    }

    /// [`SceneDraw::drawn_material_parts`] on the scene.
    fn drawn_material_parts(&self) -> (u32, u32) {
        let (_, draw) = self.halves();
        draw.drawn_material_parts()
    }

    /// [`SceneDraw::drawn_alpha_lists`] on the scene.
    fn drawn_alpha_lists(&self) -> AlphaListStats {
        let (_, draw) = self.halves();
        draw.drawn_alpha_lists()
    }

    /// [`SceneDraw::drawn_landscape_alpha`] on the scene.
    fn drawn_landscape_alpha(&self) -> LandscapeAlphaStats {
        let (_, draw) = self.halves();
        draw.drawn_landscape_alpha()
    }

    /// [`SceneDraw::drawn_alpha_order`] on the scene.
    fn drawn_alpha_order(&self) -> Vec<AlphaDraw> {
        let (_, draw) = self.halves();
        draw.drawn_alpha_order()
    }

    /// [`SceneDraw::drawn_blend_order`] on the scene.
    fn drawn_blend_order(&self) -> Vec<(AlphaDraw, f32)> {
        let (_, draw) = self.halves();
        draw.drawn_blend_order()
    }

    /// [`SceneDraw::drawn_object_cone`] on the scene.
    fn drawn_object_cone(&self) -> ObjectConeStats {
        let (_, draw) = self.halves();
        draw.drawn_object_cone()
    }

    /// [`SceneDraw::drawn_cell_statics`] on the scene.
    fn drawn_cell_statics(&self) -> CellStaticDrawStats {
        let (_, draw) = self.halves();
        draw.drawn_cell_statics()
    }

    /// [`SceneDraw::cell_static_parts`] on the scene.
    fn cell_static_parts(&self) -> Vec<CellStaticPartProbe> {
        let (_, draw) = self.halves();
        draw.cell_static_parts()
    }

    /// [`SceneDraw::drawn_cell_static_runs`] on the scene.
    fn drawn_cell_static_runs(&self) -> Vec<CellStaticRunDraw> {
        let (_, draw) = self.halves();
        draw.drawn_cell_static_runs()
    }

    /// [`SceneDraw::drawn_part_order`] on the scene.
    fn drawn_part_order(&self) -> Vec<PartSubsetDraw> {
        let (_, draw) = self.halves();
        draw.drawn_part_order()
    }

    /// [`SceneDraw::drawn_cells`] on the scene.
    fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
        let (_, draw) = self.halves();
        draw.drawn_cells()
    }

    /// [`SceneDraw::drawn_objects`] on the scene.
    fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
        let (_, draw) = self.halves();
        draw.drawn_objects()
    }

    /// [`SceneDraw::outside_view_polys`] on the scene.
    fn outside_view_polys(&self) -> Vec<Vec<(f32, f32)>> {
        let (_, draw) = self.halves();
        draw.outside_view_polys()
    }

    /// [`SceneDraw::drawn_portal_stamps`] on the scene.
    fn drawn_portal_stamps(&self) -> (u64, u64) {
        let (_, draw) = self.halves();
        draw.drawn_portal_stamps()
    }

    /// [`SceneDraw::cell_view_polys`] on the scene.
    fn cell_view_polys(&self, cell: CellId) -> Vec<Vec<(f32, f32)>> {
        let (_, draw) = self.halves();
        draw.cell_view_polys(cell)
    }

    /// [`SceneDraw::building_portal_openings`] on the scene.
    fn building_portal_openings(&self) -> Vec<(Vec3, Vec3)> {
        let (_, draw) = self.halves();
        draw.building_portal_openings()
    }

    /// [`SceneDraw::building_shell_levels`] on the scene.
    fn building_shell_levels(&self) -> Vec<(Vec3, usize)> {
        let (_, draw) = self.halves();
        draw.building_shell_levels()
    }

    /// [`SceneDraw::building_portal_screen_polygons`] on the scene.
    fn building_portal_screen_polygons(&self, width: u32, height: u32) -> Vec<Vec<(f32, f32)>> {
        let (world, draw) = self.halves();
        draw.building_portal_screen_polygons(world, width, height)
    }

    /// [`SceneDraw::indoor_outdoor_portal_screen_polygons`] on the scene.
    fn indoor_outdoor_portal_screen_polygons(
        &self,
        width: u32,
        height: u32,
    ) -> Vec<Vec<(f32, f32)>> {
        let (world, draw) = self.halves();
        draw.indoor_outdoor_portal_screen_polygons(world, width, height)
    }

    /// [`SceneDraw::cell_static_screen_boxes`] on the scene.
    fn cell_static_screen_boxes(&self, width: u32, height: u32) -> Vec<(f32, f32, f32, f32)> {
        let (world, draw) = self.halves();
        draw.cell_static_screen_boxes(world, width, height)
    }

    /// [`SceneDraw::indoor_traversal_counts`] on the scene.
    fn indoor_traversal_counts(&self) -> (usize, usize) {
        let (world, draw) = self.halves();
        draw.indoor_traversal_counts(world)
    }

    /// [`SceneDraw::indoor_outside_view_count`] on the scene.
    fn indoor_outside_view_count(&self) -> Option<usize> {
        let (world, draw) = self.halves();
        draw.indoor_outside_view_count(world)
    }

    /// [`SceneDraw::env_cell_counts`] on the scene.
    fn env_cell_counts(&self) -> (usize, usize) {
        let (world, draw) = self.halves();
        draw.env_cell_counts(world)
    }
}

/// Mutating operations shared by an owned scene and its mutable view.
pub trait SceneWrites: sealed::SceneHalvesMut {
    /// [`SceneDraw::stream`] on the scene.
    fn stream(&mut self, store: &RetailDatStore, gpu: &mut Gpu) -> Result<(), WorldError> {
        let (world, draw) = self.halves_mut();
        draw.stream(world, store, gpu)
    }

    /// [`SceneDraw::update_from_preferences`] on the scene.
    fn update_from_preferences(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
    ) -> Result<RenderPrefWork, WorldError> {
        let (world, draw) = self.halves_mut();
        draw.update_from_preferences(world, store, gpu)
    }

    /// [`SceneDraw::release_textures`] on the scene.
    fn release_textures(&mut self, gpu: &mut Gpu) -> u32 {
        let (_, draw) = self.halves_mut();
        draw.release_textures(gpu)
    }

    /// [`SceneDraw::set_force_level`] on the scene.
    fn set_force_level(&mut self, level: i32) {
        let (_, draw) = self.halves_mut();
        draw.set_force_level(level)
    }

    /// [`SceneDraw::attach_character`] on the scene.
    fn attach_character(
        &mut self,
        store: &std::sync::Arc<RetailDatStore>,
        region: &dereth_assets::Region,
        gpu: &mut Gpu,
    ) -> Result<(), WorldError> {
        let (world, draw) = self.halves_mut();
        draw.attach_character(world, store, region, gpu)
    }

    /// [`SceneDraw::sync_objects`] on the scene.
    fn sync_objects(
        &mut self,
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        stream: &mut ObjectStream,
    ) -> Result<(), WorldError> {
        let (world, draw) = self.halves_mut();
        draw.sync_objects(world, store, gpu, stream)
    }

    /// [`SceneDraw::take_sound_events`] on the scene.
    fn take_sound_events(&mut self) -> Vec<SoundTrigger> {
        let (world, draw) = self.halves_mut();
        draw.take_sound_events(world)
    }

    /// [`SceneDraw::replace_player_particle_script`] on the scene.
    fn replace_player_particle_script(&mut self, script: DataId) -> bool {
        let (world, draw) = self.halves_mut();
        draw.replace_player_particle_script(world, script)
    }

    /// [`SceneDraw::process_hooks`] on the scene.
    fn process_hooks(&mut self) {
        let (world, draw) = self.halves_mut();
        draw.process_hooks(world)
    }

    /// [`SceneDraw::update`] on the scene.
    fn update(
        &mut self,
        input: dereth_client_runtime::camera::CameraInput,
        character: dereth_client_runtime::character::CharacterInput,
        now: dereth_primitives::LocalTime,
        dt: f32,
    ) {
        let (world, draw) = self.halves_mut();
        draw.update(world, input, character, now, dt)
    }

    /// [`SceneDraw::set_weather_enabled`] on the scene.
    fn set_weather_enabled(&mut self, on: bool) {
        let (_, draw) = self.halves_mut();
        draw.set_weather_enabled(on)
    }

    /// [`SceneDraw::set_world_fog`] on the scene.
    fn set_world_fog(&mut self, on: bool) -> bool {
        let (world, draw) = self.halves_mut();
        draw.set_world_fog(world, on)
    }

    /// [`SceneDraw::set_always_daylight`] on the scene.
    fn set_always_daylight(&mut self, on: bool) -> bool {
        let (world, draw) = self.halves_mut();
        draw.set_always_daylight(world, on)
    }

    /// [`SceneDraw::apply_camera_translucency`] on the scene.
    fn apply_camera_translucency(&mut self) {
        let (world, draw) = self.halves_mut();
        draw.apply_camera_translucency(world)
    }

    /// [`SceneDraw::set_game_viewport`] on the scene.
    fn set_game_viewport(&mut self, viewport: Option<dereth_render::camera::Viewport>) {
        let (_, draw) = self.halves_mut();
        draw.set_game_viewport(viewport)
    }
}

impl SceneReads for WorldScene {}
impl SceneReads for WorldSceneRef<'_> {}
impl SceneReads for WorldSceneMut<'_> {}
impl SceneWrites for WorldScene {}
impl SceneWrites for WorldSceneMut<'_> {}

impl SceneHalves for WorldScene {
    fn halves(&self) -> (&WorldState, &SceneDraw) {
        (&self.world, &self.draw)
    }
}

impl SceneHalvesMut for WorldScene {
    fn halves_mut(&mut self) -> (&mut WorldState, &mut SceneDraw) {
        (&mut self.world, &mut self.draw)
    }
}

impl SceneHalves for WorldSceneRef<'_> {
    fn halves(&self) -> (&WorldState, &SceneDraw) {
        (self.world, self.draw)
    }
}

impl SceneHalves for WorldSceneMut<'_> {
    fn halves(&self) -> (&WorldState, &SceneDraw) {
        (self.world, self.draw)
    }
}

impl SceneHalvesMut for WorldSceneMut<'_> {
    fn halves_mut(&mut self) -> (&mut WorldState, &mut SceneDraw) {
        (self.world, self.draw)
    }
}

// What the ungated half of the client reads and writes on the drawn world. Every method is a
// forward: the runtime traits exist so that `App`
// can hold a presentation rather than a device.
macro_rules! impl_scene_reads {
    ($ty:ty) => {
        impl dereth_client_runtime::present::Scene for $ty {
            fn world(&self) -> &WorldState {
                self.halves().0
            }

            fn character(&self) -> Option<&dereth_client_runtime::character::Character> {
                self.halves().0.character.as_ref()
            }

            fn census(&self) -> dereth_client_runtime::present::SceneCensus {
                dereth_client_runtime::present::SceneCensus::from(&self.halves().1.stats)
            }

            fn server_object_count(&self) -> usize {
                self.halves().0.server_object_count()
            }

            fn viewer_cell_id(&self) -> Option<CellId> {
                self.halves().0.viewer_cell_id()
            }

            fn viewer_block(&self) -> Option<(i32, i32)> {
                self.halves().0.viewer_block()
            }

            fn loading_near_viewer(&self) -> bool {
                let (ws, draw) = self.halves();
                draw.loading_near_viewer(ws)
            }

            fn blocks_pending(&self) -> usize {
                self.halves().1.pending_slot_count()
            }

            fn interior_batches(&self) -> usize {
                let (ws, draw) = self.halves();
                draw.env_cell_counts(ws).1
            }

            fn game_date_time(&self) -> Option<(String, String)> {
                self.halves().0.game_date_time()
            }

            fn frame_rate_fps(&self) -> f32 {
                self.halves().1.degrade.frame_rate.fps()
            }

            fn degrade_meter(&self) -> (bool, f32, f32) {
                let (ws, draw) = self.halves();
                let g = draw.degrade_globals(ws);
                (g.auto_update_deg_mul, g.deg_mul, g.user_bias)
            }

            fn render_preferences(&self) -> dereth_client_runtime::render_prefs::RenderPreferences {
                self.halves().1.cfg.render
            }

            fn server_object_frame(&self, id: ObjectId) -> Option<Frame> {
                self.halves().0.server_object_frame(id)
            }

            fn take_selected_part_drawn(&self) -> bool {
                self.halves().1.take_selected_part_drawn()
            }

            fn set_selected_object_id(&self, id: Option<ObjectId>) {
                self.halves().1.set_selected_object_id(id);
            }

            fn effective_viewport(
                &self,
                width: u32,
                height: u32,
            ) -> dereth_render::camera::Viewport {
                let (ws, draw) = self.halves();
                draw.effective_viewport(ws, width, height)
            }

            fn view_params(&self, width: u32, height: u32) -> ViewParams {
                let (ws, draw) = self.halves();
                draw.view_params(ws, width, height)
            }

            fn as_pick_scene(&self) -> &dyn dereth_client_runtime::pick::PickScene {
                self
            }

            fn listener(&self) -> dereth_audio::Listener {
                self.halves().0.listener()
            }

            fn terrain_neighbourhood(&self) -> dereth_audio::TerrainNeighbourhood {
                let (ws, draw) = self.halves();
                draw.terrain_neighbourhood(ws)
            }

            fn character_sound_table(&self) -> Option<DataId> {
                self.halves().0.character_sound_table()
            }

            fn region(&self) -> &dereth_assets::Region {
                self.halves().1.region()
            }
        }
    };
}

macro_rules! impl_scene_writes {
    ($ty:ty) => {
        impl dereth_client_runtime::present::SceneMut for $ty {
            fn world_mut(&mut self) -> &mut WorldState {
                self.halves_mut().0
            }

            fn take_sound_events(&mut self) -> Vec<dereth_client_runtime::audio::SoundTrigger> {
                let (ws, draw) = self.halves_mut();
                draw.take_sound_events(ws)
            }

            fn update(
                &mut self,
                input: dereth_client_runtime::camera::CameraInput,
                character: dereth_client_runtime::character::CharacterInput,
                now: dereth_primitives::LocalTime,
                dt: f32,
            ) {
                let (ws, draw) = self.halves_mut();
                draw.update(ws, input, character, now, dt);
            }

            fn apply_camera_translucency(&mut self) {
                let (ws, draw) = self.halves_mut();
                draw.apply_camera_translucency(ws);
            }

            fn release_landscape_for_teleport(
                &mut self,
                destination: dereth_primitives::LandblockId,
            ) {
                let (ws, draw) = self.halves_mut();
                draw.release_landscape_for_teleport(ws, destination);
            }

            fn replace_player_particle_script(&mut self, script: DataId) -> bool {
                let (ws, draw) = self.halves_mut();
                draw.replace_player_particle_script(ws, script)
            }

            fn set_environment_override_state(&mut self, state: EnvironmentOverrideState) {
                let (ws, draw) = self.halves_mut();
                draw.set_environment_override_state(ws, state);
            }

            fn sync_environment_override_flags(&mut self) {
                let (ws, draw) = self.halves_mut();
                draw.sync_environment_override_flags(ws);
            }

            fn set_always_daylight(&mut self, on: bool) -> bool {
                let (ws, draw) = self.halves_mut();
                draw.set_always_daylight(ws, on)
            }

            fn set_weather_enabled(&mut self, on: bool) {
                self.halves_mut().1.set_weather_enabled(on);
            }

            fn set_world_fog(&mut self, on: bool) -> bool {
                let (ws, draw) = self.halves_mut();
                draw.set_world_fog(ws, on)
            }
        }
    };
}

impl_scene_reads!(WorldScene);
impl_scene_reads!(WorldSceneRef<'_>);
impl_scene_reads!(WorldSceneMut<'_>);
impl_scene_writes!(WorldScene);
impl_scene_writes!(WorldSceneMut<'_>);
