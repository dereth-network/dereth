//! The world scene: the landblock window, its objects and how a frame draws them.
//!
//! **This module wires; it does not implement.** Everything that decides *what* the world looks
//! like lives in the world-render crate and is called from here by name:
//!
//! | Decision | Whose |
//! |---|---|
//! | the landblock window, its LOD rings and stitch directions | `LandblockWindow` |
//! | the mesh: vertices, the diagonal split, planes, UVs, rotation | `generate_landblock_with_table` |
//! | the merged terrain texture and its cache | `TerrainMergeCache` |
//! | per-vertex terrain lighting | `bake_lighting` |
//! | which blocks and cells are drawn, in what order | `block_draw_order`, `cell_draw_order` |
//! | which of a cell's two triangles face the viewer | `visible_triangles` |
//! | the emitted vertex bytes | `triangle_vertices` |
//! | where scenery grows, and all five filters | `generate_scenery` |
//! | which cells own a building, and so grow no scenery | `SortCells` |
//!
//! and everything that decides *how* it reaches the GPU lives in the render crates: the pipeline catalogue
//! (`PipelineKey::from_surface`), the texture decode, and the projection and view matrices
//! ([`dereth_render::camera`]).
//!
//! What this crate owns is the free camera ([`crate::camera`]), the three-hop texture lookup
//! ([`crate::textures`]) and — a known gap rather than a feature — the object
//! triangulation in [`dereth_client_runtime::models`].
//!

/// The retail region record, the landblock identity helpers and [`WorldError`].
///
/// Defined in [`dereth_client_runtime::landblock`], because `land_source.rs`, `env_cells.rs` and
/// `character.rs` are core modules and all three name them. The `dereth_client::world` paths
/// (`dereth_client::world::lbi_did` and its siblings) resolve through this `pub use`.
pub use dereth_client_runtime::landblock::{
    block_xy, landblock_did, lbi_did, load_region, WorldError, DEFAULT_LANDBLOCK, DERETH_REGION,
};

/// Process-owned administrative environment presets.
///
/// The whole cluster -- `EnvironmentOverride`, [`EnvironmentOverrideState`],
/// `admin_environs_radar_blank` and `admin_environs_override` -- is defined in
/// [`dereth_client_runtime::environment`], because `present::Scene::set_environment_override_state`
/// names the handle and that trait belongs to the headless crate. The struct, its seven fields and
/// `snapshot`/`advance` are `pub` there, with no device-feature gate; the path
/// `dereth_client::world::EnvironmentOverrideState` resolves through this `pub use`.
pub use dereth_client_runtime::environment::{EnvironmentOverride, EnvironmentOverrideState};
/// What one [`WorldScene::update_from_preferences`] poll actually did.
///
/// Defined in [`dereth_client_runtime::frame_events`], because it is the payload of
/// `FrameEvent::RenderPreferencesPolled`. It is six flags and three counts and names nothing
/// else; the path `dereth_client::world::RenderPrefWork` resolves through this `pub use`.
pub use dereth_client_runtime::frame_events::RenderPrefWork;
/// What the scene is built from: `dereth_client_runtime`'s, re-exported at this path.
pub use dereth_client_runtime::scene::SceneConfig;

/// The wire `ObjDesc` as the animation runtime's one. Defined in
/// [`dereth_client_runtime::movement::to_anim_objdesc`] and re-exported here, so
/// `dereth_client::world::to_anim_objdesc` resolves.
pub use dereth_client_runtime::movement::to_anim_objdesc;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub use imp::*;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[allow(clippy::too_many_lines)]
mod imp {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    use std::sync::Arc;

    use dereth_assets::world::{CellLandblock, LandblockInfo};
    use dereth_assets::Decode;
    use dereth_client_runtime::world_stream::{LandblockWindow, SlotAction, SlotMesh};
    use dereth_dat::{DbType, RetailDatStore};
    use dereth_primitives::{CellId, DataId, Frame, Quat, TextureHandle, Vec3};
    use dereth_render::device::{
        Gpu, MergeSource, PerDrawConstants, PerFrameConstants, TerrainMergeJob,
        TerrainMergeOverlay, TerrainSplat, TerrainSplatOverlay, TextureSlot,
    };
    use dereth_render::palette::ExpandedPalette;
    use dereth_render::pso::{PipelineKey, SurfaceContext};
    use dereth_render::surface::{Surface as RenderState, SurfaceHandler};
    use dereth_render::vertex::VertexFormat;
    use dereth_render::{Cull, DrawConstants, RenderError, ViewParams, ZFunc};
    use dereth_world_render::consts::BLOCK_LENGTH;
    use dereth_world_render::land::emit::{
        detail_vertex, triangle_vertices, visible_triangles, LAND_VERTEX_STRIDE,
    };
    use dereth_world_render::land::lighting::{bake_lighting, LandscapeLighting};
    use dereth_world_render::land::merge::{
        land_texture_scale_shift, Bgra8, MergeKey, MergePlan, TerrainMergeCache,
        TerrainTextureSource,
    };
    use dereth_world_render::land::mesh::{
        generate_landblock_with_table, height_table, LandblockMesh,
    };
    use dereth_world_render::land::order::{block_draw_order, cell_draw_order};
    // The light pool, the eight slots and the D3DLIGHT9s, from the one transcription.
    use dereth_world_render::lighting::{
        ambient_render_state, calc_object_light, enabled_lights, minimize_envcell_lighting,
        minimize_object_lighting, set_color32, sunlight_light, use_sunlight_set, viewer_light,
        world_ambient, ActiveLights, D3dLight, LightInfo, LightPools, LightType, BYTE_TO_FLOAT,
        INDOOR_AMBIENT_LEVEL,
    };
    use dereth_world_render::math::V3;
    use dereth_world_render::scenery::outside_cell_index;

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
    use dereth_client_runtime::anim_assets::DatAnimAssets;
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

    use super::{
        block_xy, load_region, EnvironmentOverrideState, RenderPrefWork, SceneConfig, WorldError,
        DERETH_REGION,
    };
    #[cfg(test)]
    use super::{landblock_did, lbi_did};
    #[cfg(test)]
    use crate::camera::FreeCamera;
    use dereth_client_runtime::render_prefs::{RegionStyle, RequiredFiles};
    #[cfg(test)]
    use dereth_client_runtime::world_build::read_lbi;

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
        /// Subsets dropped because a list was already at [`dereth_world_render::consts::ALPHA_LIST_CAP`].
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
        /// A pinned governor — [`dereth_world_render::consts::PINNED_DEG_MUL`] — unless the scene
        /// asked for the live loop.
        #[must_use]
        pub fn new(auto: bool) -> Self {
            use dereth_world_render::degrade_loop::{DegradeGovernor, FramerateTargets};
            let governor = if auto {
                DegradeGovernor::automatic(FramerateTargets::default())
            } else {
                DegradeGovernor::pinned(dereth_world_render::consts::PINNED_DEG_MUL)
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
        dir: dereth_world_render::land::mesh::Direction,
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
        /// The mesh's burned-static-lights marker: `None` until
        /// the static-light burn has written the static light into the
        /// meshes' vertex colours, then the static-light count it was burned at -- the cache key
        /// the client uses: bit 7 set and `& 0x7F ==` the static-light count means
        /// "already current"), so a pool that swaps one light for another at the same count does
        /// not re-burn, exactly as retail does not.
        burned_count: Option<usize>,
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
    #[derive(Debug)]
    struct BuildingView {
        /// One-based outdoor land-cell index. Native reaches this building through that cell's
        /// sort-cell draw, after its terrain and before the nearer cells in the draw-order walk.
        cell_index: u16,
        /// `BuildInfo::frame` — the building's placement inside the block.
        /// Portal traversal runs inside this position push, so the BSP planes and polygons below are in
        /// **building** space and the viewpoint is transformed into it.
        frame: Frame,
        /// Walked per frame by
        /// `dereth_world_render::cells::portal_view::build_draw_portals_only`, because which portals it
        /// yields, and in what order, depends on where the viewer is.
        bsp: dereth_assets::common::BspTree,
        /// The building's outdoor portal list, with `other_cell_id` widened by the landblock base.
        portals: Vec<dereth_world_render::cells::portal_view::BuildingPortal>,
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
        render_shadow: crate::render_prefs::RenderPreferences,
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
        /// Same bracket: the object pass's blend-list draws (parts and particles) in device
        /// order, each with the viewer distance it was sorted by.
        frame_blend_order: std::cell::RefCell<Vec<(AlphaDraw, f32)>>,
        /// What the per-object frustum test did on the last [`WorldScene::draw`].
        frame_object_cone: std::cell::Cell<ObjectConeStats>,
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
        /// and drained by `dereth_client::interaction::use_time` into
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

    /// The scene's own sound stash, as the hook drain's sink. `dereth_client::audio::SoundTrigger`
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
        /// build's memo costs one each, which is a declared deviation and is a *leak* rather than
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
        /// Draw batches the resident interior cells' baked objects cost, and what
        /// static registration made of them on the physics side.
        pub cell_static_batches: usize,
        pub cell_statics: dereth_client_runtime::env_cells::CellStaticStats,
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
        /// that drives `update` and never calls [`crate::camera::update_viewer`] leaves the
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
        /// ([`dereth_world_render::land::fill`]).
        drawn: u32,
        tex_merge: dereth_assets::region::TexMerge,
        /// The ground's palette-shift land surface (an older dat set's software region), whose
        /// cells are composed here on the CPU; `None` for texture merging. Which of the two
        /// draws is read from the region's own land-surface record, never from the dat set's era.
        pal_shift: Option<dereth_assets::region::PalShift>,
        table: Box<[f32; dereth_world_render::consts::LAND_HEIGHT_TABLE_LEN]>,
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
        /// its own inside [`dereth_client_runtime::land_source::DatLandSource`], because that one has to be
        /// reachable from a `LandSource` behind an `Arc` and this one has to be reachable from the
        /// scene; they read the same records and share nothing else.
        cells: dereth_client_runtime::env_cells::EnvCellLoader,
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
        cells: dereth_client_runtime::env_cells::EnvCellLoader,
        /// Which ids the look's records stand for the same object as the world's.
        pub(crate) identity: Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
        /// The surfaces those records resolve to. Apart from the world's cache because the two
        /// eras hold different records under the same ids, so a memo keyed by id would hand one
        /// era's surface to the other.
        pub(crate) cache: BakeCache,
    }

    impl ObjectLook {
        /// The look of `files` (an [`RetailDatStore::object_files`] store) under `identity`, with a
        /// surface cache set up as `like`, the world's, is.
        fn new(
            files: RetailDatStore,
            interiors: Option<RetailDatStore>,
            identity: Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
            like: &BakeCache,
        ) -> Self {
            Self {
                files: Arc::new(files),
                interiors: interiors.map(Arc::new),
                cells: dereth_client_runtime::env_cells::EnvCellLoader::new(),
                identity,
                cache: BakeCache {
                    surface_translucency: like.surface_translucency,
                    image_scale: like.image_scale,
                    environment_texture_detail: like.environment_texture_detail,
                    ..BakeCache::default()
                },
            }
        }

        /// Whether a landscape object -- a setup or a model, `building` for a building shell --
        /// draws wholly with the look.
        fn takes(&self, id: DataId, building: bool) -> bool {
            self.files.portal().contains(id)
                && self.identity.same(id)
                && (!building || self.identity.same_geometry(id))
        }
    }

    /// The verdicts for the world's portal against the other era's (`files`), and the world's
    /// rooms against the other era's when its cell file is here (`interiors`), all at once: from
    /// the host's cache folder when `cache` and it holds them, timed into the log.
    fn object_identity(
        store: &RetailDatStore,
        files: &RetailDatStore,
        interiors: Option<&RetailDatStore>,
        cache: bool,
    ) -> Arc<dereth_client_runtime::object_identity::ObjectIdentity> {
        let t = web_time::Instant::now();
        let dir = if cache {
            dereth_client_runtime::object_identity::default_cache_dir()
        } else {
            None
        };
        let id = dereth_client_runtime::object_identity::ObjectIdentity::load_or_build_with(
            store.portal(),
            files.portal(),
            interiors.map(|i| (store.cell(), i.cell())),
            dir.as_deref(),
        );
        tracing::info!(
            "object identity: {} ids and {} rooms of the other era stand for the world's, ready \
             in {:.2} s",
            id.len(),
            id.rooms_len(),
            t.elapsed().as_secs_f64()
        );
        Arc::new(id)
    }

    /// The files `style` draws the world's objects with: `Ok(None)` for the world's own (no
    /// style, or the world's own era), the other era's portal first otherwise.
    ///
    /// # Errors
    /// The files `style` needs, when they are not here.
    pub fn object_files_for(
        store: &RetailDatStore,
        style: Option<RegionStyle>,
    ) -> Result<Option<RetailDatStore>, RequiredFiles> {
        let Some(style) = style else {
            return Ok(None);
        };
        let needs = style.required_files();
        let era = match needs {
            RequiredFiles::Legacy => dereth_dat::ContainerEra::PreTod,
            RequiredFiles::Modern => dereth_dat::ContainerEra::Tod,
        };
        if era == store.era() {
            return Ok(None);
        }
        store.object_files(era).map(Some).ok_or(needs)
    }

    impl std::fmt::Debug for LandContext {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("LandContext")
                .field("surfaces", &self.merge.len())
                .finish_non_exhaustive()
        }
    }

    /// The region a dat set from before Throne of Destiny carries for drawing with 3D hardware,
    /// beside the one at `0x13000000` for drawing in software. The two differ only in their
    /// sky and their land surface; a later dat set has only the one.
    pub const HARDWARE_REGION: DataId = DataId(0x130F_0000);

    /// Hand back every texture a surface cache holds, one release per slot it owns, and empty it.
    /// Returns how many were released.
    fn release_bake_cache(cache: &mut BakeCache, gpu: &mut Gpu) -> u32 {
        let mut n = 0;
        // ORDER-OK: every slot is released exactly once and no release can affect another.
        for slot in cache.by_slot.keys() {
            gpu.release_texture(TextureSlot(*slot));
            n += 1;
        }
        cache.surfaces.clear();
        cache.links.clear();
        cache.by_slot.clear();
        cache.key_clip_map.clear();
        cache.textures_uploaded = 0;
        n
    }

    /// Read and decode one region record from `files`.
    fn read_region(
        files: &RetailDatStore,
        id: DataId,
    ) -> Result<dereth_assets::Region, WorldError> {
        let bytes = files
            .read_typed(DbType::Region, id)
            .map_err(|e| WorldError::Region(id, e.to_string()))?;
        match dereth_assets::decode_any_in(files.era_of(id), DbType::Region, id, &bytes)
            .map_err(|e| WorldError::Region(id, e.to_string()))?
        {
            dereth_assets::DecodedAsset::Region(r) => Ok(r),
            other => Err(WorldError::Region(id, format!("decoded as {other:?}"))),
        }
    }

    /// The world's own region: the world files' hardware region where they have one, as the
    /// clients of the world's time loaded it when they drew with 3D hardware, and otherwise the
    /// region at `0x13000000`. Its ground and sky are what [`SceneConfig::render`]'s `ground` and
    /// `sky` of `None` draw, and everything else the region decides (scenery, sound, the calendar)
    /// is read from it whatever they choose.
    ///
    /// # Errors
    /// [`WorldError::Region`] when the record will not read or decode.
    pub fn world_region(store: &RetailDatStore) -> Result<dereth_assets::Region, WorldError> {
        if store.portal().contains(HARDWARE_REGION) {
            read_region(store, HARDWARE_REGION)
        } else {
            load_region(store)
        }
    }

    /// Why a landscape style could not be drawn.
    #[derive(Debug)]
    pub enum StyleError {
        /// Its files are not present: the world's own are the other era's, and none of its era
        /// were given beside them.
        Missing(RequiredFiles),
        /// Its region would not read or decode.
        World(WorldError),
    }

    impl From<WorldError> for StyleError {
        fn from(e: WorldError) -> Self {
            Self::World(e)
        }
    }

    /// A landscape style's region and the files its pictures and objects are read from; `None`
    /// files are the world's own.
    #[derive(Debug)]
    pub struct StyleSource {
        pub region: dereth_assets::Region,
        pub files: Option<RetailDatStore>,
    }

    /// The region `style` names, from the files of its era: an older world's own files or the
    /// presentation portal beside a later world for the two older styles, a later world's own
    /// files or the later files beside an older world for the modern one.
    ///
    /// # Errors
    /// [`StyleError::Missing`] when those files are not present or do not hold the region;
    /// [`StyleError::World`] when it will not decode.
    pub fn style_region(
        store: &RetailDatStore,
        style: RegionStyle,
    ) -> Result<StyleSource, StyleError> {
        let needs = style.required_files();
        let (files, own) = match needs {
            RequiredFiles::Legacy => (
                store.legacy_files(),
                store.era() == dereth_dat::ContainerEra::PreTod,
            ),
            RequiredFiles::Modern => (
                store.modern_files(),
                store.era() == dereth_dat::ContainerEra::Tod,
            ),
        };
        let files = files.ok_or(StyleError::Missing(needs))?;
        let id = match style {
            RegionStyle::LegacyHardware => HARDWARE_REGION,
            RegionStyle::LegacySoftware | RegionStyle::Modern => DERETH_REGION,
        };
        if !files.portal().contains(id) {
            return Err(StyleError::Missing(needs));
        }
        let region = read_region(&files, id)?;
        Ok(StyleSource {
            region,
            files: (!own).then_some(files),
        })
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
        /// ([`dereth_world_render::land::fill::drawn_terrain_types`]).
        drawn: u32,
    }

    /// The ground `style` draws over the world's `region` (`None`: the world's own). The style's
    /// land surface must be its technique: palette shifting for the older software region, texture
    /// merging for the other two.
    ///
    /// The cells' terrain words index the same terrain types in every region from the first to
    /// the last (the same names and map colours at the same indices, later regions adding to the
    /// end), and the road bits mean the same, so any land surface reads any world's cells as they
    /// are. A type the land surface has no picture for is filled from its neighbours
    /// ([`dereth_world_render::land::fill`]).
    ///
    /// # Errors
    /// As [`style_region`].
    fn ground_for(
        store: &RetailDatStore,
        region: &dereth_assets::Region,
        style: Option<RegionStyle>,
    ) -> Result<GroundChoice, StyleError> {
        let Some(style) = style else {
            return Ok(GroundChoice {
                ground: None,
                files: None,
                drawn: dereth_world_render::land::fill::drawn_terrain_types(
                    &region.land_surf,
                    region.terrain_types.len(),
                ),
            });
        };
        let source = style_region(store, style)?;
        let surf = source.region.land_surf;
        let technique = match style {
            RegionStyle::LegacySoftware => surf.pal_shift.is_some(),
            RegionStyle::LegacyHardware | RegionStyle::Modern => surf.tex_merge.is_some(),
        };
        if !technique {
            return Err(StyleError::Missing(style.required_files()));
        }
        // The types the style's own region names, which its land surface draws with pictures of
        // their own.
        let drawn = dereth_world_render::land::fill::drawn_terrain_types(
            &surf,
            source.region.terrain_types.len(),
        );
        if source.files.is_none() && surf == region.land_surf {
            return Ok(GroundChoice {
                ground: None,
                files: None,
                drawn,
            });
        }
        let mut ground = region.clone();
        ground.land_surf = surf;
        Ok(GroundChoice {
            ground: Some(ground),
            files: source.files,
            drawn,
        })
    }

    /// The sky `style` draws: the style's region, whose sky, light and fog are read, and the
    /// files to read its objects from (`None`: the world's). Both `None` for the world's own sky.
    ///
    /// # Errors
    /// As [`style_region`].
    fn sky_for(
        store: &RetailDatStore,
        region: &dereth_assets::Region,
        style: Option<RegionStyle>,
    ) -> Result<(Option<dereth_assets::Region>, Option<RetailDatStore>), StyleError> {
        let Some(style) = style else {
            return Ok((None, None));
        };
        let source = style_region(store, style)?;
        if source.region.sky_info.is_none() {
            return Err(StyleError::Missing(style.required_files()));
        }
        if source.files.is_none() && source.region.sky_info == region.sky_info {
            return Ok((None, None));
        }
        Ok((Some(source.region), source.files))
    }

    /// The land surface a region draws its ground with: texture merging, or (the software region
    /// of an older dat set) palette shifting, which composes each cell on the CPU and has no splat
    /// form. The record decides; the dat set's era does not.
    fn land_surface(
        region: &dereth_assets::Region,
    ) -> Result<
        (
            dereth_assets::region::TexMerge,
            Option<dereth_assets::region::PalShift>,
        ),
        WorldError,
    > {
        let surf = &region.land_surf;
        let pal_shift = surf.pal_shift.clone();
        let tex_merge = match (&surf.tex_merge, &pal_shift) {
            (Some(tm), _) => tm.clone(),
            (None, Some(_)) => dereth_assets::region::TexMerge {
                base_tex_size: 0,
                corner_terrain_maps: Vec::new(),
                side_terrain_maps: Vec::new(),
                road_maps: Vec::new(),
                terrain_desc: Vec::new(),
            },
            (None, None) => return Err(WorldError::MissingTerrainTexture),
        };
        Ok((tex_merge, pal_shift))
    }

    impl LandContext {
        /// Generate one block's mesh and its vertex lighting, then the block's merged
        /// land surfaces.
        ///
        /// The merge cache is shared across the whole window, so a block entering the window
        /// usually uploads only the handful of textures its own terrain pairs need.
        fn generate(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            lb: &CellLandblock,
            spec: SlotSpec,
        ) -> Result<GeneratedBlock, WorldError> {
            // A terrain type the ground has no picture for takes its neighbours' for the surfaces
            // alone; the water stays the cells' own.
            let filled = dereth_world_render::land::fill::fill_undrawn_terrain(lb, self.drawn);
            let surface_lb = filled.as_ref().unwrap_or(lb);
            let mut mesh = generate_landblock_with_table(
                surface_lb,
                self.ground.as_deref().unwrap_or(&self.region),
                &self.table,
                spec.block_x,
                spec.block_y,
                spec.lod_div,
                spec.dir,
            );
            if filled.is_some() {
                (mesh.cell_water, mesh.water_type) = dereth_world_render::land::water::calc_water(
                    &lb.terrain,
                    usize::from(mesh.side_cell_count),
                );
            }
            // `generate` recomputes the vertex lighting after every geometry rebuild.
            bake_lighting(&mut mesh, &self.lighting);

            let merge_keys: Vec<MergeKey> = mesh.cell_keys.iter().map(|&(k, _)| k).collect();
            if let Some(ps) = self.pal_shift.clone() {
                let cell_texture = self.pal_shift_composites(store, gpu, &ps, surface_lb, &mesh)?;
                return Ok((
                    mesh,
                    cell_texture,
                    merge_keys.clone(),
                    merge_keys,
                    Vec::new(),
                ));
            }
            let (cell_texture, cell_keys) = if self.composites_wanted {
                (
                    self.composites(store, gpu, &merge_keys)?,
                    merge_keys.clone(),
                )
            } else {
                (Vec::new(), Vec::new())
            };
            let cell_splat = if self.splat_wanted {
                self.splat_layers(store, gpu, &merge_keys)?
            } else {
                Vec::new()
            };
            Ok((mesh, cell_texture, cell_keys, merge_keys, cell_splat))
        }

        /// Each of `keys`' composite, taking one reference per cell on the shared cache.
        ///
        /// One reference per cell, and the key kept so the block can give that
        /// same reference back. Teardown visits every one of `side_cell_count²` cells,
        /// returning the link acquired at terrain selection: cache hits increment the
        /// reference count, while a newly created surface starts with one reference.
        fn composites(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            keys: &[MergeKey],
        ) -> Result<Vec<Option<TextureSlot>>, WorldError> {
            let ground_store = self.ground_store.clone();
            let store = ground_store.as_ref().unwrap_or(store);
            // One cached texture per distinct merge key.
            let sources = DatTerrainTextures {
                textures: TextureStore::with_environment_texture_detail(
                    store,
                    self.bake.environment_texture_detail,
                ),
                memo: RefCell::new(&mut self.sources),
            };
            let mut cell_texture = Vec::with_capacity(keys.len());
            let mut error: Option<RenderError> = None;
            for key in keys {
                let h = if self.gpu_merge && gpu.terrain_merge_supported() {
                    let resident = &mut self.gpu_merge_sources;
                    let gpu_merge = &mut self.gpu_merge;
                    let gpu = &mut *gpu;
                    let sources = &sources;
                    self.merge
                        .get_or_build_with(&self.tex_merge, *key, &mut |plan| {
                            match merge_on_device(gpu, resident, sources, plan) {
                                Ok(slot) => TextureHandle(slot.0),
                                Err(e) => {
                                    // The two paths are bit-identical, so falling back loses nothing;
                                    // it is for the rest of the session, so the line prints once.
                                    tracing::warn!(
                                        "composing the landscape on the CPU from here on: {e}"
                                    );
                                    *gpu_merge = false;
                                    let img = dereth_world_render::land::merge::execute_merge_plan(
                                        plan, sources,
                                    );
                                    let mut backend = TextureUploader {
                                        gpu: &mut *gpu,
                                        error: None,
                                    };
                                    dereth_primitives::RenderBackend::upload_texture(
                                        &mut backend,
                                        &dereth_primitives::TextureData {
                                            width: img.width,
                                            height: img.height,
                                            format: dereth_primitives::TextureFormat::Bgra8,
                                            levels: vec![img.into_bytes()],
                                        },
                                    )
                                }
                            }
                        })
                } else {
                    let mut backend = TextureUploader {
                        gpu: &mut *gpu,
                        error: None,
                    };
                    let h = self
                        .merge
                        .get_or_build(&self.tex_merge, *key, &mut backend, &sources);
                    if let Some(e) = backend.error.take() {
                        error.get_or_insert(e);
                    }
                    h
                };
                cell_texture.push((h.0 != NO_TEXTURE).then_some(TextureSlot(h.0)));
            }
            if let Some(e) = error {
                return Err(WorldError::Render(e.to_string()));
            }
            Ok(cell_texture)
        }

        /// Each cell's palette-shift surface, composed on the CPU once per key and shared through
        /// the same surface cache (one reference per cell). The texture and rotation are chosen
        /// per cell at its global position, so the first cell to need a key decides its picture,
        /// as the surface cache's first request does.
        fn pal_shift_composites(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            ps: &dereth_assets::region::PalShift,
            lb: &CellLandblock,
            mesh: &dereth_world_render::land::mesh::LandblockMesh,
        ) -> Result<Vec<Option<TextureSlot>>, WorldError> {
            use dereth_world_render::land::merge::{cell_rotation_keys, cell_x, cell_y};
            use dereth_world_render::land::palshift;
            let ground_store = self.ground_store.clone();
            let store = ground_store.as_ref().unwrap_or(store);
            let lookup = dereth_assets::texture_lookup::TextureLookup::new(store, 0);
            let side = usize::from(mesh.side_cell_count);
            let mut out = Vec::with_capacity(side * side);
            let mut error: Option<String> = None;
            for i in 0..side {
                for j in 0..side {
                    let (keys, _) = cell_rotation_keys(
                        lb,
                        self.ground.as_deref().unwrap_or(&self.region),
                        side,
                        i,
                        j,
                    );
                    let choice = palshift::select(ps, &keys, cell_x(lb, i), cell_y(lb, j));
                    let Some(texture) = ps.textures.get(choice.texture) else {
                        out.push(None);
                        continue;
                    };
                    let h = self.merge.get_or_build_keyed(choice.key, &mut || {
                        let image = lookup.resolve(texture.tex_gid).ok().and_then(|(_, rs, b)| {
                            let indices = rs.payload(&b)?.to_vec();
                            let base = rs
                                .default_palette_id
                                .and_then(|p| lookup.palette(p).ok())
                                .map(|p| p.colors_argb)
                                .unwrap_or_default();
                            let subs = palshift::sub_palettes(ps, &choice);
                            let palette = palshift::compose_palette(&base, &subs, &|id| {
                                lookup.palette(id).ok().map(|p| p.colors_argb)
                            });
                            Some((rs.width, rs.height, palshift::expand(&indices, &palette)))
                        });
                        let Some((width, height, pixels)) = image else {
                            error.get_or_insert(format!(
                                "the palette-shift texture {} does not resolve",
                                texture.tex_gid
                            ));
                            return (TextureHandle(NO_TEXTURE), 0);
                        };
                        let mut backend = TextureUploader {
                            gpu: &mut *gpu,
                            error: None,
                        };
                        let h = dereth_primitives::RenderBackend::upload_texture(
                            &mut backend,
                            &dereth_primitives::TextureData {
                                width,
                                height,
                                format: dereth_primitives::TextureFormat::Bgra8,
                                levels: vec![pixels],
                            },
                        );
                        if let Some(e) = backend.error.take() {
                            error.get_or_insert(e.to_string());
                        }
                        (h, width)
                    });
                    out.push((h.0 != NO_TEXTURE).then_some(TextureSlot(h.0)));
                }
            }
            if let Some(e) = error {
                return Err(WorldError::Render(e));
            }
            Ok(out)
        }

        /// Each of `keys`' splat layers, made once per merge key and shared.
        fn splat_layers(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            keys: &[MergeKey],
        ) -> Result<Vec<Option<Arc<TerrainSplat>>>, WorldError> {
            let ground_store = self.ground_store.clone();
            let store = ground_store.as_ref().unwrap_or(store);
            let Self {
                sources,
                splat_sources,
                splats,
                tex_merge,
                merge,
                bake,
                ..
            } = self;
            let src = DatTerrainTextures {
                textures: TextureStore::with_environment_texture_detail(
                    store,
                    bake.environment_texture_detail,
                ),
                memo: RefCell::new(sources),
            };
            let mut cells = Vec::with_capacity(keys.len());
            for key in keys {
                let splat = match splats.get(key) {
                    Some(s) => Arc::clone(s),
                    None => {
                        let plan = dereth_world_render::land::merge::merge_plan(
                            tex_merge,
                            *key,
                            merge.shift,
                        );
                        // The texels a composite at this detail level would hold for a cell,
                        // before the distance reduction the splat draw leaves to the mips.
                        let size = dereth_world_render::land::merge::merged_texture_size(
                            tex_merge.base_tex_size,
                            merge.shift,
                            1,
                        );
                        let s = Arc::new(splat_of(gpu, splat_sources, &src, &plan, size)?);
                        splats.insert(*key, Arc::clone(&s));
                        s
                    }
                };
                cells.push(Some(splat));
            }
            Ok(cells)
        }

        /// Dynamic scenery, buildings and static objects for one block, baked into
        /// **block-local** vertices.
        ///
        /// Block-local coordinates use the block id and an identity frame, letting the window scroll
        /// without touching a vertex.
        #[allow(clippy::too_many_arguments)]
        // LINT-OK: the block's identity is four of them (`lb`, `mesh`, `bx`, `by`) and they are
        // three different views of the same block that the callee needs separately;
        // `cell_statics` is the eighth and `part_degrades` the ninth.
        fn bake(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            lb: &CellLandblock,
            mesh: &LandblockMesh,
            bx: i32,
            by: i32,
            cell_statics: bool,
            part_degrades: bool,
            degrade_levels: bool,
            static_billboards: bool,
            lod_object_guard: bool,
        ) -> Result<BakedObjects, WorldError> {
            let pending_before = self.bake.pending_surfaces;
            let look_pending_before = self
                .objects
                .as_ref()
                .map_or(0, |l| l.cache.pending_surfaces);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: `read_landblock` already bounded both to 0..=0xFE. Not a float conversion.
            let block = ((bx as u16) << 8) | (by as u16);
            // **What the block holds is the world builder's; this bake draws it.** The scenery,
            // the landblock-info statics resolved to their land cells, the collision record of
            // each and the house restrictions all come from `land_content`, which a presentation
            // with no device calls too. Objects of every kind need full detail there, not scenery
            // alone: static-object and building initialization carry the same guard, so a block
            // on an outer ring is bare terrain.
            //
            // The bake turns the same placements into batches in the same order -- scenery, then
            // building shells, then statics -- and pairs every placement whose setup names a
            // default script with its collision record by index: static initialization creates
            // one physics body per placement and queues its default script on that same body.
            let LandContent {
                lbi,
                full_detail,
                placed,
                object_slots,
                land_statics,
                mut cell_restrictions,
            } = land_content(store, &self.region, lb, mesh, bx, by, lod_object_guard);

            // The scenery, the building shells and the statics, each drawn wholly from one era:
            // the objects' look when it is another era's and stands for the object
            // ([`ObjectLook::takes`]), the world's otherwise. Their collision records above are
            // the world's either way.
            let objects_visual = self.objects.is_some();
            let takes = |id: DataId, building: bool| {
                self.objects.as_ref().is_some_and(|l| l.takes(id, building))
            };
            // (object, frame, scale, building shell, drawn from the look), in bake order.
            let mut items: Vec<(DataId, Frame, f32, bool, bool)> = Vec::new();
            let mut emitters: Vec<EmitterPlacement> = Vec::new();
            for (i, p) in placed.iter().enumerate() {
                items.push((p.gfxobj, p.frame, p.scale, false, takes(p.gfxobj, false)));
                if crate::particles::has_default_script(store, p.gfxobj) {
                    // A scenery placement's collision record is `land_statics[i]`.
                    emitters.push(EmitterPlacement {
                        cell: p.cell,
                        setup: p.gfxobj,
                        frame: p.frame,
                        slot: StaticSlot::Land(i),
                        body: None,
                    });
                }
            }
            let (mut buildings, mut statics) = (0usize, 0usize);
            if let Some(info) = lbi.as_ref().filter(|_| full_detail) {
                buildings = info.buildings.len();
                statics = info.objects.len();
                for b in &info.buildings {
                    // Building drawing sets the building-part flag only for the shell.
                    // The existing baker still resolves all parts; retail draws parts[0] here.
                    items.push((b.id, b.frame, 1.0, true, takes(b.id, true)));
                }
                for (o, slot) in info.objects.iter().zip(&object_slots) {
                    items.push((o.id, o.frame, 1.0, false, takes(o.id, false)));
                    // A placement whose land-cell lookup answered nothing is destroyed during
                    // static-object initialization and never reaches registration, so it has no
                    // record to pair with.
                    if let Some((cell, index)) = *slot {
                        if crate::particles::has_default_script(store, o.id) {
                            emitters.push(EmitterPlacement {
                                cell,
                                setup: o.id,
                                frame: o.frame,
                                slot: StaticSlot::Land(index),
                                body: None,
                            });
                        }
                    }
                }
            }
            // The world's side first, then the look's, each through its own files and cache; the
            // look's degrading placements follow the world's, so its batches' chunks are moved up
            // by the world's count.
            let mut baked: BakedGroups = (Vec::new(), Vec::new(), Vec::new());
            for from_look in [false, true] {
                if !items.iter().any(|it| it.4 == from_look) {
                    continue;
                }
                let (draw_store, draw_cache): (&RetailDatStore, &mut BakeCache) =
                    match (from_look, self.objects.as_mut()) {
                        (true, Some(look)) => (&*look.files, &mut look.cache),
                        _ => (store, &mut self.bake),
                    };
                let mut baker = ObjectBaker::new(
                    draw_store,
                    draw_cache,
                    part_degrades,
                    degrade_levels,
                    static_billboards,
                );
                baker.from_look = from_look;
                baker.defer = if self.defer_mips {
                    Some(&mut self.mip_worker)
                } else {
                    None
                };
                for (id, frame, scale, building, _) in items.iter().filter(|it| it.4 == from_look) {
                    baker.add_object_kind(*id, frame, *scale, *building);
                }
                let (mut opaque, mut blended, degrade) = baker
                    .finish(gpu)
                    .map_err(|e| WorldError::Render(e.to_string()))?;
                // LINT-OK: a placement count bounded by the block's part count. Not a float.
                #[allow(clippy::cast_possible_truncation)]
                let offset = baked.2.len() as u32;
                for b in opaque.iter_mut().chain(blended.iter_mut()) {
                    for c in &mut b.chunks {
                        if c.placement != NO_PLACEMENT {
                            c.placement += offset;
                        }
                    }
                }
                baked.0.append(&mut opaque);
                baked.1.append(&mut blended);
                baked.2.extend(degrade);
            }
            let (opaque, blended, degrade) = baked;
            // A building's rooms keep the world's look with a shell that keeps it.
            let shell_list = lbi.as_ref().map_or(&[][..], |i| &i.buildings[..]);
            let shells: Vec<bool> = shell_list
                .iter()
                .map(|b| self.objects.as_ref().is_some_and(|l| l.takes(b.id, true)))
                .collect();
            // The interior cells' own baked objects are found on the same pass that
            // builds their meshes, and their scripted ones join this block's `emitters` — a
            // dungeon brazier's flame comes exactly as a lamp post's
            // does outdoors.
            let (env_cells, cell_statics, cell_lights) = self.bake_env_cells(
                store,
                gpu,
                block,
                (shell_list, &shells),
                cell_statics,
                part_degrades,
                degrade_levels,
                static_billboards,
                &mut emitters,
                &mut cell_restrictions,
            )?;
            // The outdoor portal pass into a building's interior is
            // reached only through a sorted cell's building link,
            // and building initialization creates no building object
            // on a coarse block, so there is nothing there to open. Gated with the shell.
            let building_views = lbi
                .as_ref()
                .filter(|_| full_detail)
                .map_or_else(Vec::new, |info| bake_building_views(store, block, info));
            let look_pending = self
                .objects
                .as_ref()
                .map_or(0, |l| l.cache.pending_surfaces);
            Ok(BakedObjects {
                textures_pending: self.bake.pending_surfaces != pending_before
                    || look_pending != look_pending_before,
                objects_visual,
                opaque,
                blended,
                degrade,
                env_cells,
                building_views,
                scenery: placed.len(),
                buildings,
                statics,
                sim: BlockStatics {
                    cell_statics,
                    land_statics,
                    cell_restrictions,
                    statics_registered: false,
                    restrictions_registered: false,
                },
                cell_lights,
                emitters,
                hosts: Vec::new(),
                hosts_spawned: false,
            })
        }

        /// Build one block's interior cells for drawing, including each cell mesh.
        ///
        /// Each cell record's polygons are triangulated against the **cell's own**
        /// surface list and composed into block-local space by the cell's placement frame.
        ///
        /// The second half of a cell is its static tables,
        /// pictures, stairs and fireplaces the cell bakes in. They are baked into per-cell batches
        /// rather than into the block's, because a cell's objects are drawn only when the portal
        /// traversal reaches that cell (cell drawing's second loop,
        /// the per-cell object draw) — dropping them into `BlockDraw::opaque` would draw
        /// a dungeon's whole furniture through every wall. The frames are **not** composed with
        /// the cell's own: see [`dereth_client_runtime::env_cells::CellStatic`].
        ///
        /// With another era's look ([`ObjectLook`]), a room the verdicts say is the same draws
        /// from that era's record of it unless it belongs to one of `shells`' buildings whose
        /// shell keeps the world's look (`shells`: the block's buildings and whether each shell
        /// takes the look).
        #[allow(clippy::too_many_arguments)]
        // LINT-OK: the eighth is `degrade_levels`, which joins `part_degrades`
        // and `want_statics` as a `SceneConfig` control the bake has to be told about; the ninth
        // the shells' verdicts, which the rooms follow.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        fn bake_env_cells(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            block: u16,
            shells: (&[dereth_assets::world::BuildInfo], &[bool]),
            want_statics: bool,
            part_degrades: bool,
            degrade_levels: bool,
            static_billboards: bool,
            emitters: &mut Vec<EmitterPlacement>,
            restrictions: &mut Vec<(CellId, dereth_primitives::ObjectId)>,
        ) -> Result<
            (
                Vec<EnvCellDraw>,
                Vec<dereth_client_runtime::env_cells::CellStatic>,
                Vec<CellLightObj>,
            ),
            WorldError,
        > {
            let decoded = self.cells.load_block(store, block);
            if decoded.is_empty() {
                return Ok((Vec::new(), Vec::new(), Vec::new()));
            }
            // The interior half of the house barrier and every cell's statics are the world
            // builder's (`interior_content`); this bake draws the cells and the statics.
            let (interior_restrictions, per_cell_statics) =
                interior_content(&decoded, want_statics);
            restrictions.extend(interior_restrictions);
            let textures = TextureStore::with_environment_texture_detail(
                store,
                self.bake.environment_texture_detail,
            );
            // The other era's rooms: its files, and the rooms held to the world's look by their
            // shells.
            let interiors = self.objects.as_ref().and_then(|l| l.interiors.clone());
            let look_files = self.objects.as_ref().map(|l| Arc::clone(&l.files));
            let look_textures = interiors.as_deref().map(|i| {
                TextureStore::with_environment_texture_detail(
                    i,
                    self.bake.environment_texture_detail,
                )
            });
            let held: HashSet<u32> = if interiors.is_some() {
                let (buildings, takes) = shells;
                dereth_client_runtime::env_cells::building_cells(block, buildings, &decoded)
                    .into_iter()
                    .zip(takes)
                    .filter(|(_, takes)| !**takes)
                    .flat_map(|(cells, _)| cells.into_iter().map(|c| c.0))
                    .collect()
            } else {
                HashSet::new()
            };
            let mut out = Vec::with_capacity(decoded.len());
            let mut all_statics: Vec<dereth_client_runtime::env_cells::CellStatic> = Vec::new();
            let mut all_lights: Vec<CellLightObj> = Vec::new();
            // Per static id, decoded once per block -- a dungeon
            // stands the same wall torch forty times.
            let mut setup_lights: HashMap<DataId, Vec<LightInfo>> = HashMap::new();
            for (d, statics) in decoded.iter().zip(per_cell_statics) {
                // The vertices are emitted white here and `burn_static_lighting` writes
                // the static-light burn's colour over them once the pool is
                // known; `minimize_envcell_lighting` picks the dynamic lights at draw.
                //
                // The room is the other era's record of it when the verdicts allow: its
                // geometry, the same as the world's, triangulated against that era's own surface
                // list and drawn through that era's files and cache.
                let room = match (self.objects.as_mut(), interiors.as_deref()) {
                    (Some(look), Some(files))
                        if look.identity.same_room(d.id.0) && !held.contains(&d.id.0) =>
                    {
                        look.cells.load_cell(files, d.id)
                    }
                    _ => None,
                };
                let defer = if self.defer_mips {
                    Some(&mut self.mip_worker)
                } else {
                    None
                };
                let meshes = match (
                    &room,
                    self.objects.as_mut(),
                    interiors.as_deref(),
                    &look_textures,
                ) {
                    (Some(r), Some(look), Some(files), Some(tex)) => {
                        let groups = dereth_client_runtime::models::triangulate_cell(
                            &r.structure,
                            &r.cell.surfaces,
                        );
                        build_meshes_with(
                            files,
                            &mut look.cache,
                            tex,
                            gpu,
                            &groups,
                            None,
                            Some(&d.cell.frame),
                            defer,
                        )
                    }
                    _ => {
                        let groups = dereth_client_runtime::models::triangulate_cell(
                            &d.structure,
                            &d.cell.surfaces,
                        );
                        build_meshes_with(
                            store,
                            &mut self.bake,
                            &textures,
                            gpu,
                            &groups,
                            None,
                            Some(&d.cell.frame),
                            defer,
                        )
                    }
                }
                .map_err(|e| WorldError::Render(e.to_string()))?;
                let from_look = room.is_some();
                let base = d.id.0 & 0xFFFF_0000;
                let portals = d
                    .cell
                    .portals
                    .iter()
                    .filter_map(|p| {
                        let poly = d.structure.polygons.get(p.polygon_id as usize)?;
                        let vertices: Vec<Vec3> = poly
                            .vertex_ids
                            .iter()
                            .filter_map(|&i| d.structure.vertex_array.vertices.get(i as usize))
                            .map(|v| v.position)
                            .collect();
                        // Plane construction is the physics crate's, and the traversal needs
                        // the same plane physics uses.
                        let plane = dereth_physics::geom::Polygon::new(vertices.clone()).plane;
                        Some(EnvPortal {
                            // Portal polarity is `portal_side = ((~flags) >> 1) & 1`.
                            portal_side: u8::from((!p.flags >> 1) & 1 != 0),
                            other_cell_id: if p.other_cell_id == 0xFFFF_FFFF {
                                dereth_world_render::cells::portal_view::OUTDOORS
                            } else {
                                base | (p.other_cell_id & 0xFFFF)
                            },
                            other_portal_id: i32::from(p.other_portal_id),
                            exact_match: p.flags & 1 != 0,
                            vertices,
                            plane_normal: plane.normal,
                            plane_d: plane.d,
                        })
                    })
                    .collect();
                // Interior static-object initialization's draw half. One
                // `ObjectBaker` per cell, so the cell owns its own batches; `add_object` is the
                // same call landscape-static placements go through, at the
                // placement's own block-local frame and `gfxobj_scale` 1 (an environment-cell static
                // carries no scale, exactly as a landblock-record static does not). With
                // `SceneConfig::cell_statics` off the cell has no statics at all: no triangles, no
                // emitters, no bodies, and none of the furniture's textures.
                // This cell's placements land at `all_statics[statics_base + i]`
                // — the `extend_from_slice` at the end of the cell's block is what puts them
                // there — and `all_statics` is returned as `BlockStatics::cell_statics`, which is
                // the slice `init_cell_statics` hands to `CellStaticObjects::init`. So
                // `statics_base + i` is the client's `i` in `static_objects[i]`.
                let statics_base = all_statics.len();
                // In a room drawn from the other era, each piece of furniture draws wholly from
                // the era the objects' verdicts give it, through that era's files and cache: the
                // world's pieces first, then the look's, whose degrading placements follow the
                // world's in the cell's list.
                let mut statics_opaque = Vec::new();
                let mut statics_blended = Vec::new();
                let mut cell_degrade = Vec::new();
                for side_look in [false, true] {
                    let mine: Vec<&dereth_client_runtime::env_cells::CellStatic> = statics
                        .iter()
                        .filter(|s| {
                            let takes = from_look
                                && self.objects.as_ref().is_some_and(|l| l.takes(s.id, false));
                            takes == side_look
                        })
                        .collect();
                    if mine.is_empty() {
                        continue;
                    }
                    let (draw_store, draw_cache): (&RetailDatStore, &mut BakeCache) =
                        match (side_look, self.objects.as_mut(), look_files.as_deref()) {
                            (true, Some(look), Some(files)) => (files, &mut look.cache),
                            _ => (store, &mut self.bake),
                        };
                    let mut baker = ObjectBaker::new(
                        draw_store,
                        draw_cache,
                        part_degrades,
                        degrade_levels,
                        static_billboards,
                    );
                    baker.from_look = side_look;
                    baker.defer = if self.defer_mips {
                        Some(&mut self.mip_worker)
                    } else {
                        None
                    };
                    for s in mine {
                        baker.add_object(s.id, &s.frame, 1.0);
                    }
                    let (mut opaque, mut blended, degrade) = baker
                        .finish(gpu)
                        .map_err(|e| WorldError::Render(e.to_string()))?;
                    // LINT-OK: a placement count bounded by the cell's part count. Not a float.
                    #[allow(clippy::cast_possible_truncation)]
                    let offset = cell_degrade.len() as u32;
                    for b in opaque.iter_mut().chain(blended.iter_mut()) {
                        for c in &mut b.chunks {
                            if c.placement != NO_PLACEMENT {
                                c.placement += offset;
                            }
                        }
                    }
                    statics_opaque.append(&mut opaque);
                    statics_blended.append(&mut blended);
                    cell_degrade.extend(degrade);
                }
                for (i, s) in statics.iter().enumerate() {
                    if crate::particles::has_default_script(store, s.id) {
                        emitters.push(EmitterPlacement {
                            cell: s.cell,
                            setup: s.id,
                            frame: s.frame,
                            slot: StaticSlot::Cell(statics_base + i),
                            body: None,
                        });
                    }
                    // Light initialization for the static:
                    // Object creation with `(id, 0, 0)` leaves the constructor's `LIGHTING_ON_PS` set and
                    // object initialization sets `STATIC_PS`, so every entry becomes a
                    // light object with `state |= 1` at the object's frame, registered with the cell.
                    let lights = setup_lights
                        .entry(s.id)
                        .or_insert_with(|| read_setup_lights(store, s.id));
                    for info in lights.iter() {
                        all_lights.push(CellLightObj {
                            cell: d.id,
                            info: *info,
                            frame: s.frame,
                            is_static: true,
                        });
                    }
                }
                all_statics.extend_from_slice(&statics);
                out.push(EnvCellDraw {
                    id: d.id,
                    frame: d.cell.frame,
                    portals,
                    meshes,
                    from_look,
                    statics: statics_opaque,
                    statics_blended,
                    degrade: cell_degrade,
                    burned_count: None,
                });
            }
            Ok((out, all_statics, all_lights))
        }
    }

    /// Decode one `0x02……` setup's light entries in key order; a
    /// bare `0x01……` graphics-object id has none.
    /// The serialized light entry has no `type` field, so this path chooses point/type 0
    /// and converts the packed colour word into float components.
    fn read_setup_lights(store: &RetailDatStore, id: DataId) -> Vec<LightInfo> {
        if dereth_dat::divine_type(id) != Some(DbType::Setup) {
            return Vec::new();
        }
        let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
            return Vec::new();
        };
        let Ok(setup) = dereth_assets::Setup::decode_payload_in(store.era_of(id), id, &bytes)
        else {
            return Vec::new();
        };
        setup
            .lights
            .values()
            .map(|l| LightInfo {
                light_type: LightType::Point,
                offset: l.frame,
                viewerspace_location: Vec3::ZERO,
                color: set_color32(l.color_argb),
                intensity: l.intensity,
                falloff: l.falloff,
                cone_angle: l.cone_angle,
            })
            .collect()
    }

    /// The same for a live object's `SetupData` (`dereth_animation`'s decode of the same record).
    fn setup_data_lights(setup: &dereth_animation::data::SetupData) -> Vec<LightInfo> {
        setup
            .lights
            .values()
            .map(|l| LightInfo {
                light_type: LightType::Point,
                offset: l.frame,
                viewerspace_location: Vec3::ZERO,
                color: set_color32(l.color_argb),
                intensity: l.intensity,
                falloff: l.falloff,
                cone_angle: l.cone_angle,
            })
            .collect()
    }

    /// Build the render batch for one cell's
    /// meshes: every **static** light of the pool, transformed into
    /// the mesh's space and accumulated per vertex with `calc_point_light`, then
    /// `v.diffuse = 0xFF000000 | byte(r) << 16 | byte(g) << 8 | byte(b)`.
    ///
    /// The mesh space here is the **block's** (the cell frame is folded into the vertices)
    /// rather than the cell's, so the light is brought into block space by subtracting the block
    /// origin instead of `globaltolocal(cell frame)`; the two are one rigid motion apart and the
    /// arithmetic per vertex is the same function of the same distances. The alpha byte is left
    /// as the bake stamped it; the client's burn forces it to `0xFF` and takes the alpha from
    /// the default material's `Diffuse.a` (1.0) instead.
    fn burn_env_cell(cell: &mut EnvCellDraw, pools: &LightPools, origin: (f32, f32)) {
        let locals: Vec<LightInfo> = pools
            .statics
            .iter()
            .map(|l| {
                let mut i = l.info;
                i.offset.origin = Vec3::new(
                    i.offset.origin.x - origin.0,
                    i.offset.origin.y - origin.1,
                    i.offset.origin.z,
                );
                i
            })
            .collect();
        for m in &mut cell.meshes {
            for v in m.vertices.as_chunks_mut::<OBJECT_VERTEX_STRIDE>().0 {
                let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
                let pos = Vec3::new(f(0), f(4), f(8));
                let normal = Vec3::new(f(12), f(16), f(20));
                let mut c = [0.0f32; 3];
                for li in &locals {
                    if li.light_type == LightType::Point {
                        dereth_world_render::lighting::calc_point_light(pos, normal, li, &mut c);
                    }
                }
                let o = OBJECT_DIFFUSE_OFFSET;
                // Little-endian `0xAARRGGBB`: B, G, R, A in memory.
                v[o] = burn_byte(c[2]);
                v[o + 1] = burn_byte(c[1]);
                v[o + 2] = burn_byte(c[0]);
            }
        }
    }

    /// The `byte(c)` of the burn: `c * 255` truncated to an integer, clamped to a byte.
    fn burn_byte(c: f32) -> u8 {
        // LINT-OK: clamped to 0..=255 before the cast. Not a float conversion by `as`.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            dereth_primitives::num::to_i32(c * 255.0).clamp(0, 255) as u8
        }
    }

    /// As the shader sees it: the
    /// enabled slots' `D3DLIGHT9`s into the per-draw block, with `D3DRS_LIGHTING` on and the
    /// material's Emissive as the subset draw resolves it -- the surface's `luminosity`
    /// when positive, else what the bound material carries. `emissive_from_vertex`
    /// is the burned-in env-cell branch (emissive colour taken from the vertex).
    pub(crate) fn bind_lights(
        world: &mut PerDrawConstants,
        lights: &[D3dLight],
        emissive: f32,
        emissive_from_vertex: bool,
    ) {
        let n = lights
            .len()
            .min(dereth_world_render::lighting::HARDWARE_LIGHT_SLOTS);
        // LINT-OK: at most eight. Not a float conversion of anything measured.
        #[allow(clippy::cast_precision_loss)]
        {
            world.lighting_params = [
                1.0,
                n as f32,
                emissive,
                if emissive_from_vertex { 1.0 } else { 0.0 },
            ];
        }
        for (k, l) in lights.iter().take(n).enumerate() {
            let p = if l.light_type == dereth_world_render::lighting::D3DLIGHT_DIRECTIONAL {
                l.direction
            } else {
                l.position
            };
            world.light_pos[k] = [p[0], p[1], p[2], l.range];
            // LINT-OK: a three-valued enum. Not a float conversion of anything measured.
            #[allow(clippy::cast_precision_loss)]
            {
                world.light_diffuse[k] = [
                    l.diffuse[0],
                    l.diffuse[1],
                    l.diffuse[2],
                    l.light_type as f32,
                ];
            }
        }
    }

    /// Sutherland–Hodgman against the single plane `w > ε`, in homogeneous clip space.
    ///
    /// The one plane cannot skip, and the only one
    /// [`WorldScene::building_portal_screen_polygons`] needs: an opening entirely off the side of
    /// the screen projects to a polygon off the side of the screen, which the pixel mask simply
    /// does not reach, but one straddling the eye plane projects to nonsense unless it is cut.
    fn clip_to_near_plane(poly: &[glam::Vec4]) -> Vec<glam::Vec4> {
        const EPS: f32 = 1.0e-4;
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let (ain, bin) = (a.w > EPS, b.w > EPS);
            if ain {
                out.push(a);
            }
            if ain != bin {
                let t = (EPS - a.w) / (b.w - a.w);
                out.push(a + (b - a) * t);
            }
        }
        out
    }

    /// The portal half of building initialization.
    ///
    /// Building data keeps `BuildInfo::portals` as the outdoor portal list and wraps
    /// `BuildInfo::id` in a one-part setup whose graphics object carries the **drawing** BSP. This
    /// method joins the two: each entry is `(polygon index, portal_index)`.
    ///
    /// Three polarity details are the same three as for a cell portal:
    /// `portal_side = ((~flags) >> 1) & 1`, `other_cell_id` is the **low 16 bits** and needs the
    /// landblock base OR-ed in, and a portal index is an index and not an id.
    ///
    /// A `BuildInfo` whose graphics object has no drawing BSP, or no portal node in it, yields nothing:
    /// that is a building with no interior, which is most of the world's walls and bridges.
    fn bake_building_views(
        store: &RetailDatStore,
        block: u16,
        info: &LandblockInfo,
    ) -> Vec<BuildingView> {
        use dereth_world_render::cells::portal_view::BuildingPortal as BldPortal;

        let base = u32::from(block) << 16;
        let mut out = Vec::new();
        for b in &info.buildings {
            if b.portals.is_empty() {
                continue;
            }
            let Ok(bytes) = store.read_typed(DbType::GfxObj, b.id) else {
                continue;
            };
            let Ok(g) = dereth_assets::GfxObj::decode_payload_in(store.era_of(b.id), b.id, &bytes)
            else {
                continue;
            };
            let Some(bsp) = g.drawing_bsp.clone() else {
                continue;
            };
            // Only the polygons the BSP's portal nodes name; a 226-polygon shell names two.
            let mut portal_polygons: BTreeMap<usize, BuildingPortalPolygon> = BTreeMap::new();
            for node in &bsp.nodes {
                for &(poly, _) in &node.in_portals {
                    let Ok(i) = usize::try_from(poly) else {
                        continue;
                    };
                    let Some(p) = g.polygons.get(i) else { continue };
                    let vertices: Vec<Vec3> = p
                        .vertex_ids
                        .iter()
                        .filter_map(|&v| g.vertex_array.vertices.get(v as usize))
                        .map(|v| v.position)
                        .collect();
                    if vertices.len() < 3 {
                        continue;
                    }
                    // Plane construction is the physics crate's, and the sidedness test has to read the same
                    // plane that the physics side does.
                    let plane = dereth_physics::geom::Polygon::new(vertices.clone()).plane;
                    portal_polygons.insert(
                        i,
                        BuildingPortalPolygon {
                            plane_normal: plane.normal,
                            plane_d: plane.d,
                            vertices,
                        },
                    );
                }
            }
            if portal_polygons.is_empty() {
                continue;
            }
            let portals = b
                .portals
                .iter()
                .map(|p| BldPortal {
                    portal_side: u8::from((!p.flags >> 1) & 1 != 0),
                    other_cell_id: base | u32::from(p.other_cell_id),
                    other_portal_id: i32::from(p.other_portal_id),
                    stab_list: p
                        .stab_list
                        .iter()
                        .map(|&c| CellId(base | u32::from(c)))
                        .collect(),
                })
                .collect();
            out.push(BuildingView {
                cell_index: outside_cell_index(b.frame.origin.x, b.frame.origin.y),
                frame: b.frame,
                bsp,
                portals,
                portal_polygons,
            });
        }
        out
    }

    /// The three memos an [`ObjectBaker`] would otherwise hold for the length of one bake, kept for
    /// the length of the session because a block can enter the window at any time.
    pub(crate) struct BakeCache {
        // ORDER-OK: a decode memo, only ever looked up.
        parts: HashMap<DataId, Vec<dereth_client_runtime::models::ModelPart>>,
        // ORDER-OK: as above.
        geometry: HashMap<DataId, Vec<dereth_client_runtime::models::SurfaceGroup>>,
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
        /// **The counter the declared deviation owes.** Distinct colour words this
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

    impl std::fmt::Debug for BakeCache {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BakeCache")
                .field("surfaces", &self.surfaces.len())
                .field("slots", &self.by_slot.len())
                .finish_non_exhaustive()
        }
    }

    impl Default for BakeCache {
        fn default() -> Self {
            Self {
                parts: HashMap::new(),
                geometry: HashMap::new(),
                surfaces: HashMap::new(),
                links: HashMap::new(),
                by_slot: HashMap::new(),
                key_clip_map: HashMap::new(),
                palettised: HashMap::new(),
                draws: HashMap::new(),
                degrades: HashMap::new(),
                sort_centers: HashMap::new(),
                drawing_spheres: HashMap::new(),
                parts_not_drawn: 0,
                palette_range_failures: 0,
                palette_missing: 0,
                // The `TextureScale` preference's initial value is
                // `FULL_RES`; `Render.EnvironmentTextureDetail`'s registered default is 1,
                // which `max(v,1)-1` also makes `FULL_RES`, so the two agree on a fresh install.
                image_scale: dereth_render::texture::ImageScale::FullRes,
                pending_surfaces: 0,
                environment_texture_detail: crate::render_prefs::RenderPreferences::default()
                    .environment_texture_detail,
                last_texture_built: None,
                texture_uploads: 0,
                texture_upload_texels: 0,
                texture_source_texels: 0,
                textures_downscaled: 0,
                textures_uploaded: 0,
                unowned_releases: 0,
                texture_key_hits: 0,
                solid_texel_key_hits: 0,
                solid_texels_uploaded: 0,
                solid_colour_words: std::collections::BTreeSet::new(),
                clipmap_key_conflicts: 0,
                clipmap_conflicts: Vec::new(),
                surfaces_resolved: 0,
                surfaces_translucent: 0,
                // The client always writes surface setup's `curr_alpha`; the switch is a control
                // for the differential, so the default is the client's own behaviour and a
                // `BakeCache` nobody configures honours translucency.
                surface_translucency: true,
            }
        }
    }

    impl BakeCache {
        /// [`dereth_client_runtime::models::draws_at_near_band`], memoised per graphics object.
        pub(crate) fn draws_at_near_band(
            &mut self,
            store: &RetailDatStore,
            gfxobj: DataId,
        ) -> bool {
            *self
                .draws
                .entry(gfxobj)
                .or_insert_with(|| dereth_client_runtime::models::draws_at_near_band(store, gfxobj))
        }

        /// The `GfxObjDegradeInfo` named by a graphics object's `flags & 8`, memoised and shared.
        ///
        /// `None` for an object with no record — the native null-record arm makes a one-entry
        /// `gfxobj[]` holding the object itself —
        /// and also for a record with a single level, which cannot switch and is cheaper drawn as
        /// ordinary geometry.
        pub(crate) fn degrade_record(
            &mut self,
            store: &RetailDatStore,
            gfxobj: DataId,
        ) -> Option<Arc<dereth_assets::motion::GfxObjDegradeInfo>> {
            self.degrades
                .entry(gfxobj)
                .or_insert_with(|| {
                    let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
                    let obj = dereth_assets::GfxObj::decode_payload_in(
                        store.era_of(gfxobj),
                        gfxobj,
                        &bytes,
                    )
                    .ok()?;
                    let did = obj.did_degrade?;
                    let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
                    let info = dereth_assets::GfxObjDegradeInfo::decode_payload_in(
                        store.era_of(did),
                        did,
                        &bytes,
                    )
                    .ok()?;
                    (info.degrades.len() > 1).then(|| Arc::new(info))
                })
                .clone()
        }

        /// A graphics object's level-zero sort centre,
        /// memoised. The viewer-distance update reads
        /// the graphics object's sort centre, i.e. the **level-0** mesh's, before it picks a level.
        pub(crate) fn sort_center(&mut self, store: &RetailDatStore, gfxobj: DataId) -> Vec3 {
            *self.sort_centers.entry(gfxobj).or_insert_with(|| {
                let Ok(bytes) = store.read_typed(DbType::GfxObj, gfxobj) else {
                    return Vec3::ZERO;
                };
                dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes)
                    .map_or(Vec3::ZERO, |o| o.sort_center)
            })
        }

        /// A graphics object's drawing sphere,
        /// memoised exactly as [`Self::sort_center`] is.
        ///
        /// It goes through [`dereth_client_runtime::object_physics::drawing_sphere`] — the **one** transcription of
        /// the sphere at the root of the drawing tree — rather than repeating
        /// `drawing_bsp.nodes[0].sphere` here. The collision setup in `object_physics` and the
        /// selection ray in `pick.rs` read it too: one fork with three readers, so a wrong
        /// answer is wrong in all three places at once and a mutation of it reddens all three.
        ///
        /// `None` is a real state and not a decode failure: a graphics object stores a sphere only
        /// when there is a drawing BSP to take one from.
        pub(crate) fn drawing_sphere(
            &mut self,
            store: &RetailDatStore,
            gfxobj: DataId,
        ) -> Option<(Vec3, f32)> {
            *self.drawing_spheres.entry(gfxobj).or_insert_with(|| {
                let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
                let o =
                    dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes)
                        .ok()?;
                dereth_client_runtime::object_physics::drawing_sphere(&o)
                    .map(|s| (s.center, s.radius))
            })
        }

        /// Surface setup's answer, memoised. It depends only on the surface record and the two polygon
        /// flags, so a wall texture shared by forty interior cells is uploaded once.
        ///
        /// The device's descriptor heap is finite (32,768 pairs); a town's env cells alone
        /// exhaust a 2,048-texture heap without this, and the failure
        /// is `descriptor heap exhausted` several blocks into a walk rather than at load.
        /// **Every call takes one link**, hit or miss, when the answer carries a texture. That is
        /// the keyed texture cache's shape: *"if the key is non-zero and already in the texture
        /// table, take a reference and return it"*. The caller owns that link
        /// and must return it through [`Self::release_group_texture`] when the mesh it built goes
        /// away, or hold it for the life of the scene (which is what the terrain, the statics and
        /// the local body do, and what [`WorldScene::release_textures`] settles at teardown).
        pub(crate) fn resolve(
            &mut self,
            store: &RetailDatStore,
            textures: &TextureStore<'_>,
            gpu: &mut Gpu,
            key: GroupKey,
        ) -> Result<ResolvedSurface, RenderError> {
            let r = self.resolve_with(store, textures, gpu, key, None)?;
            Ok(r.expect("a resolve that defers nothing is never pending"))
        }

        /// [`Self::resolve`], asking `defer` for a compressed texture's mip chain rather than
        /// building it here. `None` when that chain is still being built: nothing is remembered and
        /// no link is taken, so the caller throws its work away and resolves the surface again
        /// later. See [`crate::mip_worker`].
        pub(crate) fn resolve_with(
            &mut self,
            store: &RetailDatStore,
            textures: &TextureStore<'_>,
            gpu: &mut Gpu,
            key: GroupKey,
            defer: Option<&mut crate::mip_worker::MipWorker>,
        ) -> Result<Option<ResolvedSurface>, RenderError> {
            if let Some(r) = self.surfaces.get(&key).copied() {
                if let Some(slot) = r.texture {
                    *self.links.entry(slot.0).or_insert(0) += 1;
                }
                return Ok(Some(r));
            }
            // `image_scale` is at the moment
            // the device texture is created, and `built` is what actually went to the device.
            let mut built = None;
            let Some(r) = resolve_surface(
                store,
                textures,
                gpu,
                &key,
                self.image_scale,
                &mut built,
                defer,
            )?
            else {
                self.pending_surfaces += 1;
                return Ok(None);
            };
            if let Some((w, h, src_w, src_h)) = built {
                self.last_texture_built = Some((w, h, self.image_scale as u32));
                self.texture_uploads += 1;
                self.texture_upload_texels += u64::from(w) * u64::from(h);
                self.texture_source_texels += u64::from(src_w) * u64::from(src_h);
                if (w, h) != (src_w, src_h) {
                    self.textures_downscaled += 1;
                }
            }
            self.palette_range_failures += r.palette_failures;
            self.palette_missing += u32::from(r.palette_missing);
            // Counted on the **miss** path: this is a census of distinct surface
            // groups, not of draws.
            self.surfaces_resolved += 1;
            self.surfaces_translucent += u32::from(r.vertex_alpha != 0xFF);
            if let Some(slot) = r.texture {
                // `upload_texture_keyed` has already answered
                // the combined-texture creation's question. If it handed back a slot this memo
                // already owns, it incremented the image texture's reference — but this memo
                // deliberately holds exactly **one** image-texture link per slot however many
                // groups name it (which is
                // what [`WorldScene::release_textures`] balances against), so the extra reference
                // goes straight back and the group's link is counted here instead.
                match self.by_slot.entry(slot.0) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        gpu.release_texture(slot);
                        e.get_mut().push(key.clone());
                        // Two counters, not one: see [`ResolvedSurface::solid_texel`]. A single
                        // total over both transcriptions cannot fail when one of them is
                        // unwired.
                        if r.solid_texel {
                            self.solid_texel_key_hits += 1;
                        } else {
                            self.texture_key_hits += 1;
                        }
                        *self.links.entry(slot.0).or_insert(0) += 1;
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(vec![key.clone()]);
                        self.links.insert(slot.0, 1);
                        self.textures_uploaded += 1;
                        // Its own counter, on the miss arm, so the declared
                        // deviation's population is a measurement rather than an assumption. One
                        // counter per transcription: the textured half is counted by
                        // `textures_uploaded` minus this.
                        if r.solid_texel {
                            self.solid_texels_uploaded += 1;
                            // The *word*, as distinct from the *upload*. `texture_key`
                            // for this arm is `TextureKey::solid_color(argb)`, so its payload is
                            // the colour word the solid-colour texture setter would have been handed.
                            self.solid_colour_words.insert(
                                u32::try_from(r.texture_key.raw() & 0xFFFF_FFFF).unwrap_or(0),
                            );
                        }
                    }
                }
                if !r.texture_key.is_uncached() {
                    match self.key_clip_map.entry(r.texture_key) {
                        std::collections::hash_map::Entry::Occupied(e) => {
                            if *e.get() != r.clip_map {
                                self.clipmap_key_conflicts += 1;
                                // Keep the first few, with the texture id the
                                // group named, so the difference the bit makes can be measured
                                // rather than assumed either way.
                                if self.clipmap_conflicts.len() < CLIPMAP_CONFLICT_SAMPLE {
                                    self.clipmap_conflicts.push(ClipMapConflict {
                                        texture: key.appearance.texture.or(key.surface),
                                        surface: key.surface,
                                        first_clip_map: *e.get(),
                                        second_clip_map: r.clip_map,
                                    });
                                }
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(e) => {
                            e.insert(r.clip_map);
                        }
                    }
                }
            }
            self.surfaces.insert(key, r);
            Ok(Some(r))
        }

        /// Return one of the links [`Self::resolve`] handed out.
        ///
        /// Retail decrements only while the count is above the cache's own
        /// link and frees the object once it is not. This build's memo holds no link of
        /// its own, so the equivalent edge is **zero**: the last consumer's release frees the entry.
        ///
        /// Retail would then put the object on a per-type free list
        /// bounded by a per-type maximum (`DB_TYPE_SURFACE` 200, `DB_TYPE_SURFACETEXTURE`
        /// 400, `DB_TYPE_GFXOBJ` 200), trimming
        /// the **oldest** by timestamp when the free count *exceeds* the cap
        /// only when the free count is strictly greater than the cap. **That retention is
        /// deliberately not transcribed here**, a declared deviation: retail's caps are per
        /// *dat record*, and this memo's unit
        /// is a resolved surface *group*, which has no counterpart on that side and therefore no
        /// cap to copy. Retaining nothing is the strict end of the same edge.
        ///
        /// Returns `true` when the entry left the cache and its descriptor pair was handed back.
        pub(crate) fn release_group_texture(&mut self, gpu: &mut Gpu, slot: TextureSlot) -> bool {
            let Some(n) = self.links.get_mut(&slot.0) else {
                // A slot this cache never owned: a handle from a previous device, or a double
                // release. Tolerated and counted rather than fatal, exactly as
                // `Gpu::release_texture` treats the same case, because every caller is a teardown.
                self.unowned_releases += 1;
                return false;
            };
            *n -= 1;
            if *n > 0 {
                return false;
            }
            self.links.remove(&slot.0);
            // Every memo entry that named this texture goes with it. They are
            // one image texture between them, so the last consumer of the last of them is the last
            // consumer of all of them.
            for key in self.by_slot.remove(&slot.0).unwrap_or_default() {
                if let Some(r) = self.surfaces.remove(&key) {
                    self.key_clip_map.remove(&r.texture_key);
                }
            }
            self.textures_uploaded = self.textures_uploaded.saturating_sub(1);
            gpu.release_texture(slot);
            true
        }

        /// How many consumers hold a slot, for the tests.
        #[cfg(test)]
        pub(crate) fn link_count(&self, slot: TextureSlot) -> Option<u32> {
            self.links.get(&slot.0).copied()
        }

        /// Whether a shift palette can change anything about this surface's texture, memoised.
        ///
        /// Palette application refuses anything that is not `PFID_P8` or `PFID_INDEX16`,
        /// so a surface record backed by a DXT image is untouched by an `ObjDesc`'s palette — and
        /// keying its cache entry by the palette anyway would upload one copy of it per outfit.
        /// The descriptor heap is finite (32,768 pairs); that is not a saving to
        /// leave on the table.
        fn palettised(&mut self, textures: &TextureStore<'_>, id: DataId) -> bool {
            if let Some(hit) = self.palettised.get(&id) {
                return *hit;
            }
            let v = textures.is_palettised(id).unwrap_or(false);
            self.palettised.insert(id, v);
            v
        }

        /// The effective [`SurfaceAppearance`] for one surface record under one part's overrides.
        ///
        /// Texture-id substitution keys on the surface's **original**
        /// `SurfaceTexture` id, while palette application applies the shift palette
        /// to every surface the part owns. Both are dropped here when they cannot change a pixel,
        /// so that an unaffected surface keeps sharing one cache entry across every object.
        pub(crate) fn appearance_for(
            &mut self,
            store: &RetailDatStore,
            textures: &TextureStore<'_>,
            surface: Option<DataId>,
            overrides: Option<&dereth_animation::parts::SurfaceOverrides>,
        ) -> SurfaceAppearance {
            let (Some(sid), Some(ov)) = (surface, overrides) else {
                return SurfaceAppearance::default();
            };
            let Some(decoded) = read_surface(store, sid) else {
                return SurfaceAppearance::default();
            };
            let orig = decoded.orig_texture_id;
            let texture = orig.and_then(|o| {
                ov.texture_maps
                    .iter()
                    .find(|(old, _)| *old == o)
                    .map(|(_, n)| *n)
            });
            let effective = texture.or(orig);
            let palette = match (ov.shift_palette, effective) {
                (Some(pal), Some(tex)) if self.palettised(textures, tex) => {
                    Some(PaletteComposition {
                        base: pal,
                        ranges: ov
                            .subpalettes
                            .iter()
                            .map(|r| (r.palette_set, r.offset, r.length))
                            .collect(),
                        from_look: textures.look_ranges().to_vec(),
                    })
                }
                _ => None,
            };
            SurfaceAppearance { texture, palette }
        }
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

    impl PaletteComposition {
        /// Build the palette. `None` when the base palette is missing; the count is how many ranges
        /// could not be applied, which is a data or layout error rather than bad input — hence
        /// [`SceneStats::palette_range_failures`], which the acceptance test asserts is zero.
        fn build(&self, textures: &TextureStore<'_>) -> Option<(ExpandedPalette, u32)> {
            let mut p = textures.palette(self.base)?.make_modified();
            let mut failed = 0u32;
            for (i, &(sub, offset, length)) in self.ranges.iter().enumerate() {
                // Each entry in the range is copied from the same index of the sub-palette, so
                // the source is read at the **same absolute indices**, not from the sub-palette's
                // start. Offsets and lengths are in 8-entry units, so both are multiplied by 8.
                let src = if self.from_look.get(i).copied().unwrap_or(false) {
                    textures.look_palette(sub)
                } else {
                    textures.palette(sub)
                };
                let Some(src) = src else {
                    failed += 1;
                    continue;
                };
                let start = (offset as usize) * dereth_render::palette::PALETTE_REPLICATION;
                let Some(tail) = src.0.get(start..) else {
                    failed += 1;
                    continue;
                };
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: both are wire bytes widened to u32 on the way in. Not a float
                // conversion.
                let ok = p.apply_subpalette(offset as u16, length as u16, tail);
                if !ok {
                    failed += 1;
                }
            }
            Some((p, failed))
        }
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

    /// Build `plan`'s composite with the device's compositor, making each source it names
    /// resident on first use. A missing alpha map drops its overlay and a missing texture is the
    /// debug colour, exactly as [`dereth_world_render::land::merge::execute_merge_plan`] does, so the
    /// result is bit-identical to the CPU composite.
    fn merge_on_device(
        gpu: &mut Gpu,
        resident: &mut HashMap<DataId, Option<MergeSource>>,
        src: &dyn TerrainTextureSource,
        plan: &MergePlan,
    ) -> Result<TextureSlot, RenderError> {
        let mut get = |id: DataId| -> Result<Option<MergeSource>, RenderError> {
            if let Some(r) = resident.get(&id) {
                return Ok(*r);
            }
            let r = match src.image(id) {
                Some(img) => Some(gpu.upload_merge_source(
                    img.width,
                    img.height,
                    img.pixels.as_flattened(),
                )?),
                None => None,
            };
            resident.insert(id, r);
            Ok(r)
        };
        let base = match plan.base {
            Some(id) => get(id)?,
            None => None,
        };
        let mut overlays = Vec::with_capacity(plan.overlays.len());
        for o in &plan.overlays {
            let Some(alpha) = get(o.alpha)? else { continue };
            let tex = match o.tex {
                Some(id) => get(id)?,
                None => None,
            };
            overlays.push(TerrainMergeOverlay {
                alpha,
                rotation: o.rotation as u32,
                tex,
                tiling: o.tiling,
            });
        }
        let job = TerrainMergeJob {
            size: plan.size,
            base,
            base_tiling: plan.base_tiling,
            overlays,
        };
        gpu.merge_terrain_texture(dereth_render::TextureKey::UNCACHED, &job)
    }

    /// `plan`'s layers as a splat draw's textures, uploading each source as an ordinary texture
    /// on first use.
    fn splat_of(
        gpu: &mut Gpu,
        resident: &mut HashMap<DataId, Option<TextureSlot>>,
        src: &dyn TerrainTextureSource,
        plan: &MergePlan,
        size: u32,
    ) -> Result<TerrainSplat, WorldError> {
        // Each source is uploaded at the texels the composite at `size` carries of it
        // (`Render.LandscapeTextureDetail`), so the setting lowers the splat draw's detail as it
        // does the composites'. A change of the setting flushes these, as it does the composites.
        let mut get = |id: DataId, tiling: u32| -> Result<Option<TextureSlot>, WorldError> {
            if let Some(r) = resident.get(&id) {
                return Ok(*r);
            }
            let r = match src.image(id) {
                Some(img) => {
                    let shrunk =
                        dereth_world_render::land::merge::source_at_scale(&img, size, tiling);
                    let img = shrunk.as_ref().unwrap_or(&img);
                    Some(
                        gpu.upload_imgtex_keyed(
                            dereth_render::TextureKey::UNCACHED,
                            &dereth_primitives::TextureData {
                                width: img.width,
                                height: img.height,
                                format: dereth_primitives::TextureFormat::Bgra8,
                                levels: vec![img.pixels.as_flattened().to_vec()],
                            },
                        )
                        .map_err(|e| WorldError::Render(e.to_string()))?,
                    )
                }
                None => None,
            };
            resident.insert(id, r);
            Ok(r)
        };
        let base = match plan.base {
            Some(id) => get(id, plan.base_tiling)?,
            None => None,
        };
        let mut overlays = Vec::with_capacity(plan.overlays.len());
        for o in &plan.overlays {
            let Some(alpha) = get(o.alpha, 1)? else {
                continue;
            };
            let tex = match o.tex {
                Some(id) => get(id, o.tiling)?,
                None => None,
            };
            overlays.push(TerrainSplatOverlay {
                alpha,
                rotation: o.rotation as u32,
                tex,
                tiling: o.tiling,
            });
        }
        Ok(TerrainSplat {
            base,
            base_tiling: plan.base_tiling,
            overlays,
        })
    }

    /// A [`TerrainTextureSource`] over the retail dats, memoising decodes.
    ///
    /// `fill_temp_tex_buffer` asks for the same four or five terrain tiles for every one of the
    /// few hundred distinct merge keys, so without the memo the DXT expansion dominates load time.
    /// The memo is [`LandContext::sources`], which outlives the block being generated.
    struct DatTerrainTextures<'a> {
        textures: TextureStore<'a>,
        memo: RefCell<&'a mut HashMap<DataId, Option<Arc<Bgra8>>>>,
    }

    impl TerrainTextureSource for DatTerrainTextures<'_> {
        fn image(&self, id: DataId) -> Option<Arc<Bgra8>> {
            let mut memo = self.memo.borrow_mut();
            memo.entry(id)
                .or_insert_with(|| self.textures.bgra8(id).ok().map(Arc::new))
                .clone()
        }
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

    fn blend_override_channel(base: u8, target: u8, transition: f32) -> u8 {
        // The client converts both bytes to extended precision, multiplies by the stored f32
        // transition, subtracts, and converts to an integer. Using f64 here retains the extended-precision
        // intermediate instead of rounding at each Rust
        // f32 operator; both operands and the stored transition still have their native widths.
        let base = f64::from(base);
        let target = f64::from(target);
        let value = base - (base - target) * f64::from(transition);
        let value = dereth_primitives::num::to_i32_f64(value).clamp(0, 255);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            value as u8
        }
    }

    fn blend_override_scalar(base: f32, target: f32, transition: f32) -> f32 {
        // The client's extended-precision expression stores only its final result back to the f32 landscape field.
        #[allow(clippy::cast_possible_truncation)]
        {
            (f64::from(base) - (f64::from(base) - f64::from(target)) * f64::from(transition)) as f32
        }
    }

    fn blend_override_color(base: u32, target: u32, transition: f32) -> u32 {
        let channel = |shift: u32| {
            u32::from(blend_override_channel(
                u8::try_from((base >> shift) & 0xFF_u32).unwrap_or(0),
                u8::try_from((target >> shift) & 0xFF_u32).unwrap_or(0),
                transition,
            )) << shift
        };
        channel(24) | channel(16) | channel(8) | channel(0)
    }

    fn color_rgb(color: u32) -> [u8; 3] {
        [
            u8::try_from((color >> 16) & 0xFF).unwrap_or(0),
            u8::try_from((color >> 8) & 0xFF).unwrap_or(0),
            u8::try_from(color & 0xFF).unwrap_or(0),
        ]
    }

    impl dereth_primitives::RenderBackend for TextureUploader<'_> {
        fn upload_mesh(
            &mut self,
            _m: &dereth_primitives::MeshData,
        ) -> dereth_primitives::MeshHandle {
            debug_assert!(false, "the terrain path submits its own vertices");
            dereth_primitives::MeshHandle(0)
        }

        fn upload_texture(&mut self, t: &dereth_primitives::TextureData) -> TextureHandle {
            // Generated landscape textures are uncached, including the separate runtime AUTOGEN copy.
            match self
                .gpu
                .upload_imgtex_keyed(dereth_render::TextureKey::UNCACHED, t)
            {
                Ok(slot) => TextureHandle(slot.0),
                Err(e) => {
                    if self.error.is_none() {
                        self.error = Some(e);
                    }
                    TextureHandle(NO_TEXTURE)
                }
            }
        }

        fn draw(&mut self, _batch: &dereth_primitives::DrawBatch) {
            debug_assert!(false, "the terrain path submits its own draws");
        }
    }

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

        /// …and the mutable one.
        pub fn view_mut(&mut self) -> WorldSceneMut<'_> {
            WorldSceneMut {
                world: &mut self.world,
                draw: &mut self.draw,
            }
        }

        /// [`SceneDraw::world_fog_state`] on this scene.
        pub fn world_fog_state(&self) -> dereth_render::camera::FogParams {
            self.draw.world_fog_state(&self.world)
        }

        /// [`SceneDraw::world_fog`] on this scene.
        pub fn world_fog(&self) -> dereth_render::camera::FogParams {
            self.draw.world_fog()
        }

        /// [`SceneDraw::stream`] on this scene.
        pub fn stream(&mut self, store: &RetailDatStore, gpu: &mut Gpu) -> Result<(), WorldError> {
            self.draw.stream(&mut self.world, store, gpu)
        }

        /// [`SceneDraw::update_from_preferences`] on this scene.
        pub fn update_from_preferences(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
        ) -> Result<RenderPrefWork, WorldError> {
            self.draw
                .update_from_preferences(&mut self.world, store, gpu)
        }

        /// [`SceneDraw::detail_texturing`] on this scene.
        pub fn detail_texturing(&self) -> dereth_world_render::detail::DetailTexturing {
            self.draw.detail_texturing()
        }

        /// [`SceneDraw::release_textures`] on this scene.
        pub fn release_textures(&mut self, gpu: &mut Gpu) -> u32 {
            self.draw.release_textures(gpu)
        }

        /// [`SceneDraw::live_appearances`] on this scene.
        pub fn live_appearances(&self) -> usize {
            self.draw.live_appearances()
        }

        /// [`SceneDraw::clipmap_conflicts`] on this scene.
        pub fn clipmap_conflicts(&self) -> &[ClipMapConflict] {
            self.draw.clipmap_conflicts()
        }

        /// [`SceneDraw::appearance_link_counts`] on this scene.
        pub fn appearance_link_counts(&self) -> Vec<usize> {
            self.draw.appearance_link_counts()
        }

        /// [`SceneDraw::cell_static_batches`] on this scene.
        pub fn cell_static_batches(&self, cell: CellId) -> usize {
            self.draw.cell_static_batches(cell)
        }

        /// [`SceneDraw::resident_blocks`] on this scene.
        pub fn resident_blocks(&self) -> usize {
            self.draw.resident_blocks()
        }

        /// [`SceneDraw::deferred_surfaces`] on this scene.
        #[must_use]
        pub fn deferred_surfaces(&self) -> u64 {
            self.draw.deferred_surfaces()
        }

        /// [`SceneDraw::pending_slot_count`] on this scene.
        pub fn pending_slot_count(&self) -> usize {
            self.draw.pending_slot_count()
        }

        /// [`SceneDraw::released_block_count`] on this scene.
        pub fn released_block_count(&self) -> usize {
            self.draw.released_block_count()
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
        pub const fn render_shadow(&self) -> crate::render_prefs::RenderPreferences {
            self.draw.render_shadow()
        }

        /// [`SceneDraw::block_bake`] on this scene.
        pub fn block_bake(&self, block: (i32, i32)) -> Option<BlockBake> {
            self.draw.block_bake(block)
        }

        /// [`SceneDraw::window_rings`] on this scene.
        pub fn window_rings(&self) -> Vec<WindowRing> {
            self.draw.window_rings(&self.world)
        }

        /// [`SceneDraw::alpha_list_batches`] on this scene.
        pub fn alpha_list_batches(&self) -> usize {
            self.draw.alpha_list_batches()
        }

        /// [`SceneDraw::part_degrade_probe`] on this scene.
        pub fn part_degrade_probe(&self) -> Vec<PartLevelProbe> {
            self.draw.part_degrade_probe(&self.world)
        }

        /// [`SceneDraw::degrade_probe`] on this scene.
        pub fn degrade_probe(&self) -> Vec<StaticLevelProbe> {
            self.draw.degrade_probe(&self.world)
        }

        /// [`SceneDraw::set_force_level`] on this scene.
        pub fn set_force_level(&mut self, level: i32) {
            self.draw.set_force_level(level)
        }

        /// [`SceneDraw::force_level`] on this scene.
        pub fn force_level(&self) -> i32 {
            self.draw.force_level()
        }

        /// [`SceneDraw::attach_character`] on this scene.
        pub fn attach_character(
            &mut self,
            store: &std::sync::Arc<RetailDatStore>,
            region: &dereth_assets::Region,
            gpu: &mut Gpu,
        ) -> Result<(), WorldError> {
            self.draw
                .attach_character(&mut self.world, store, region, gpu)
        }

        /// [`SceneDraw::sync_objects`] on this scene.
        pub fn sync_objects(
            &mut self,
            store: &Arc<RetailDatStore>,
            gpu: &mut Gpu,
            stream: &mut ObjectStream,
        ) -> Result<(), WorldError> {
            self.draw.sync_objects(&mut self.world, store, gpu, stream)
        }

        /// [`SceneDraw::host_pending_events`] on this scene.
        pub fn host_pending_events(&self) -> usize {
            self.draw.host_pending_events()
        }

        /// [`SceneDraw::host_bodies`] on this scene.
        pub fn host_bodies(&self) -> Vec<HostBody> {
            self.draw.host_bodies(&self.world)
        }

        /// [`SceneDraw::region`] on this scene.
        pub fn region(&self) -> &dereth_assets::Region {
            self.draw.region()
        }

        /// [`SceneDraw::character_built_from`] on this scene.
        pub fn character_built_from(&self) -> &[DataId] {
            self.draw.character_built_from()
        }

        /// [`SceneDraw::character_part_textures`] on this scene.
        pub fn character_part_textures(&self) -> Vec<Vec<u32>> {
            self.draw.character_part_textures()
        }

        /// [`SceneDraw::terrain_neighbourhood`] on this scene.
        pub fn terrain_neighbourhood(&self) -> dereth_audio::TerrainNeighbourhood {
            self.draw.terrain_neighbourhood(&self.world)
        }

        /// [`SceneDraw::take_sound_events`] on this scene.
        pub fn take_sound_events(&mut self) -> Vec<SoundTrigger> {
            self.draw.take_sound_events(&mut self.world)
        }

        /// [`SceneDraw::replace_player_particle_script`] on this scene.
        pub fn replace_player_particle_script(&mut self, script: DataId) -> bool {
            self.draw
                .replace_player_particle_script(&mut self.world, script)
        }

        /// [`SceneDraw::process_hooks`] on this scene.
        pub fn process_hooks(&mut self) {
            self.draw.process_hooks(&mut self.world)
        }

        /// [`SceneDraw::target_projection`] on this scene.
        pub fn target_projection(
            &self,
            id: ObjectId,
            viewport: (u32, u32),
        ) -> Option<dereth_client_contract::target::Projection> {
            self.draw.target_projection(&self.world, id, viewport)
        }

        /// [`SceneDraw::upload_reservation`] on this scene.
        pub fn upload_reservation(&self) -> usize {
            self.draw.upload_reservation()
        }

        /// [`SceneDraw::reserve_upload_arena`] on this scene.
        pub fn reserve_upload_arena(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            self.draw.reserve_upload_arena(gpu)
        }

        /// [`SceneDraw::update`] on this scene.
        pub fn update(
            &mut self,
            input: crate::camera::CameraInput,
            character: dereth_client_runtime::character::CharacterInput,
            now: dereth_primitives::LocalTime,
            dt: f32,
        ) {
            self.draw.update(&mut self.world, input, character, now, dt)
        }

        /// [`SceneDraw::degrade_globals`] on this scene.
        pub fn degrade_globals(&self) -> dereth_world_render::objects::degrade::DegradeGlobals {
            self.draw.degrade_globals(&self.world)
        }

        /// [`SceneDraw::set_weather_enabled`] on this scene.
        pub fn set_weather_enabled(&mut self, on: bool) {
            self.draw.set_weather_enabled(on)
        }

        /// [`SceneDraw::weather_enabled`] on this scene.
        pub fn weather_enabled(&self) -> Option<bool> {
            self.draw.weather_enabled()
        }

        /// [`SceneDraw::set_world_fog`] on this scene.
        pub fn set_world_fog(&mut self, on: bool) -> bool {
            self.draw.set_world_fog(&mut self.world, on)
        }

        /// [`SceneDraw::set_always_daylight`] on this scene.
        pub fn set_always_daylight(&mut self, on: bool) -> bool {
            self.draw.set_always_daylight(&mut self.world, on)
        }

        /// [`SceneDraw::viewer_block_colours`] on this scene.
        #[must_use]
        pub fn viewer_block_colours(&self) -> Option<Vec<[u8; 3]>> {
            self.draw.viewer_block_colours(&self.world)
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

        /// [`SceneDraw::landscape_lighting`] on this scene.
        pub fn landscape_lighting(&self) -> LandscapeLighting {
            self.draw.landscape_lighting()
        }

        /// [`SceneDraw::apply_camera_translucency`] on this scene.
        pub fn apply_camera_translucency(&mut self) {
            self.draw.apply_camera_translucency(&mut self.world)
        }

        /// [`SceneDraw::view_params`] on this scene.
        pub fn view_params(&self, width: u32, height: u32) -> ViewParams {
            self.draw.view_params(&self.world, width, height)
        }

        /// [`SceneDraw::set_game_viewport`] on this scene.
        pub fn set_game_viewport(&mut self, viewport: Option<dereth_render::camera::Viewport>) {
            self.draw.set_game_viewport(viewport)
        }

        /// [`SceneDraw::effective_viewport`] on this scene.
        pub fn effective_viewport(
            &self,
            width: u32,
            height: u32,
        ) -> dereth_render::camera::Viewport {
            self.draw.effective_viewport(&self.world, width, height)
        }

        /// [`SceneDraw::set_selected_object_id`] on this scene.
        pub fn set_selected_object_id(&self, id: Option<ObjectId>) {
            self.draw.set_selected_object_id(id)
        }

        /// [`SceneDraw::take_selected_part_drawn`] on this scene.
        pub fn take_selected_part_drawn(&self) -> bool {
            self.draw.take_selected_part_drawn()
        }

        /// [`SceneDraw::clear_selected_part_drawn`] on this scene.
        pub fn clear_selected_part_drawn(&self) {
            self.draw.clear_selected_part_drawn()
        }

        /// [`SceneDraw::sun_light`] on this scene.
        pub fn sun_light(&self) -> Option<D3dLight> {
            self.draw.sun_light()
        }

        /// [`SceneDraw::world_ambient_color`] on this scene.
        pub fn world_ambient_color(&self) -> [f32; 3] {
            self.draw.world_ambient_color(&self.world)
        }

        /// [`SceneDraw::object_light_set`] on this scene.
        pub fn object_light_set(&self, centre: Vec3, radius: f32, outdoors: bool) -> Vec<D3dLight> {
            self.draw.object_light_set(centre, radius, outdoors)
        }

        /// [`SceneDraw::envcell_light_set`] on this scene.
        pub fn envcell_light_set(&self) -> Vec<D3dLight> {
            self.draw.envcell_light_set()
        }

        /// [`SceneDraw::light_pools`] on this scene.
        pub fn light_pools(&self) -> &LightPools {
            self.draw.light_pools()
        }

        /// [`SceneDraw::cell_light_objects`] on this scene.
        pub fn cell_light_objects(&self) -> usize {
            self.draw.cell_light_objects()
        }

        /// [`SceneDraw::env_cell_burned_vertices`] on this scene.
        pub fn env_cell_burned_vertices(&self, cell: CellId) -> Vec<(Vec3, Vec3, [u8; 3])> {
            self.draw.env_cell_burned_vertices(cell)
        }

        /// [`SceneDraw::env_cell_static_light_probe`] on this scene.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        pub fn env_cell_static_light_probe(
            &self,
            cell: CellId,
        ) -> Vec<(Vec3, f32, usize, usize, usize, f32, bool, bool)> {
            self.draw.env_cell_static_light_probe(cell)
        }

        /// [`SceneDraw::cell_block_origin`] on this scene.
        pub fn cell_block_origin(&self, cell: CellId) -> Option<(f32, f32)> {
            self.draw.cell_block_origin(cell)
        }

        /// [`SceneDraw::draw`] on this scene.
        pub fn draw(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            self.draw.draw(&self.world, gpu)
        }

        /// [`SceneDraw::emitter_degrade_probe`] on this scene.
        pub fn emitter_degrade_probe(&self) -> Vec<EmitterDegrade> {
            self.draw.emitter_degrade_probe(&self.world)
        }

        /// [`SceneDraw::emitter_host_origins`] on this scene.
        pub fn emitter_host_origins(&self) -> Vec<Vec3> {
            self.draw.emitter_host_origins(&self.world)
        }

        /// [`SceneDraw::drawn_particles`] on this scene.
        pub fn drawn_particles(&self) -> crate::particles::ParticleStats {
            self.draw.drawn_particles()
        }

        /// [`SceneDraw::drawn_material_parts`] on this scene.
        pub fn drawn_material_parts(&self) -> (u32, u32) {
            self.draw.drawn_material_parts()
        }

        /// [`SceneDraw::drawn_alpha_lists`] on this scene.
        pub fn drawn_alpha_lists(&self) -> AlphaListStats {
            self.draw.drawn_alpha_lists()
        }

        /// [`SceneDraw::drawn_landscape_alpha`] on this scene.
        pub fn drawn_landscape_alpha(&self) -> LandscapeAlphaStats {
            self.draw.drawn_landscape_alpha()
        }

        /// [`SceneDraw::drawn_alpha_order`] on this scene.
        pub fn drawn_alpha_order(&self) -> Vec<AlphaDraw> {
            self.draw.drawn_alpha_order()
        }

        /// [`SceneDraw::drawn_blend_order`] on this scene.
        pub fn drawn_blend_order(&self) -> Vec<(AlphaDraw, f32)> {
            self.draw.drawn_blend_order()
        }

        /// [`SceneDraw::drawn_object_cone`] on this scene.
        pub fn drawn_object_cone(&self) -> ObjectConeStats {
            self.draw.drawn_object_cone()
        }

        /// [`SceneDraw::drawn_part_order`] on this scene.
        pub fn drawn_part_order(&self) -> Vec<PartSubsetDraw> {
            self.draw.drawn_part_order()
        }

        /// [`SceneDraw::drawn_cells`] on this scene.
        pub fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
            self.draw.drawn_cells()
        }

        /// [`SceneDraw::drawn_objects`] on this scene.
        pub fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
            self.draw.drawn_objects()
        }

        /// [`SceneDraw::outside_view_polys`] on this scene.
        pub fn outside_view_polys(&self) -> Vec<Vec<(f32, f32)>> {
            self.draw.outside_view_polys()
        }

        /// [`SceneDraw::drawn_portal_stamps`] on this scene.
        pub fn drawn_portal_stamps(&self) -> (u64, u64) {
            self.draw.drawn_portal_stamps()
        }

        /// [`SceneDraw::cell_view_polys`] on this scene.
        pub fn cell_view_polys(&self, cell: CellId) -> Vec<Vec<(f32, f32)>> {
            self.draw.cell_view_polys(cell)
        }

        /// [`SceneDraw::building_portal_openings`] on this scene.
        pub fn building_portal_openings(&self) -> Vec<(Vec3, Vec3)> {
            self.draw.building_portal_openings()
        }

        /// [`SceneDraw::building_portal_screen_polygons`] on this scene.
        pub fn building_portal_screen_polygons(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            self.draw
                .building_portal_screen_polygons(&self.world, width, height)
        }

        /// [`SceneDraw::indoor_outdoor_portal_screen_polygons`] on this scene.
        pub fn indoor_outdoor_portal_screen_polygons(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            self.draw
                .indoor_outdoor_portal_screen_polygons(&self.world, width, height)
        }

        /// [`SceneDraw::cell_static_screen_boxes`] on this scene.
        pub fn cell_static_screen_boxes(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<(f32, f32, f32, f32)> {
            self.draw
                .cell_static_screen_boxes(&self.world, width, height)
        }

        /// [`SceneDraw::indoor_traversal_counts`] on this scene.
        pub fn indoor_traversal_counts(&self) -> (usize, usize) {
            self.draw.indoor_traversal_counts(&self.world)
        }

        /// [`SceneDraw::indoor_outside_view_count`] on this scene.
        pub fn indoor_outside_view_count(&self) -> Option<usize> {
            self.draw.indoor_outside_view_count(&self.world)
        }

        /// [`SceneDraw::env_cell_counts`] on this scene.
        pub fn env_cell_counts(&self) -> (usize, usize) {
            self.draw.env_cell_counts(&self.world)
        }
    }

    impl SceneDraw {
        /// The drawing half of a server object, which every object in
        /// `world.objects` has under the same id: the two maps gain and lose an id together.
        ///
        /// # Panics
        /// When `id` has no drawing half: something added it to `world.objects`, or removed its
        /// drawing half, outside [`Self::prepare_object_dispatch`], the one place that does both.
        fn object_draw(&self, id: ObjectId) -> &SceneObject {
            match self.object_draws.get(&id) {
                Some(d) => d,
                None => panic!(
                    "object {id:?} is in the world state but has no drawing half: world objects \
                     and their drawing halves are added and removed together by the object \
                     dispatch, and something changed one map without the other"
                ),
            }
        }

        /// Debug builds check that `world.objects` and [`Self::object_draws`] hold
        /// exactly the same ids. [`Self::prepare_object_dispatch`] is the only code that adds or
        /// removes either, and it keeps them in step; `WorldState`'s fields are public, so this
        /// catches a change to one map made elsewhere at the next dispatch rather than as a
        /// missing drawing half in the draw path. Nothing runs in release builds.
        fn debug_check_object_halves(&self, ws: &WorldState) {
            debug_assert!(
                ws.objects.keys().eq(self.object_draws.keys()),
                "world objects and their drawing halves are out of step: {} world objects, {} \
                 drawing halves; only in the world state: {:?}; only drawn: {:?}",
                ws.objects.len(),
                self.object_draws.len(),
                ws.objects
                    .keys()
                    .filter(|id| !self.object_draws.contains_key(id))
                    .take(8)
                    .collect::<Vec<_>>(),
                self.object_draws
                    .keys()
                    .filter(|id| !ws.objects.contains_key(id))
                    .take(8)
                    .collect::<Vec<_>>(),
            );
        }

        /// Build the whole static scene. Every texture is created here, before the first frame.
        ///
        /// # Errors
        /// [`WorldError`] when the region, the landblock or a device resource is unavailable.
        pub fn load(
            store: &RetailDatStore,
            gpu: &mut Gpu,
            cfg: SceneConfig,
        ) -> Result<(Self, WorldState), WorldError> {
            Self::load_with_identity(store, gpu, cfg, None)
        }

        /// [`Self::load`], with the object identity verdicts the application already has for
        /// this store (`identity`), which the other era's look is drawn with. Without them and
        /// with [`SceneConfig::object_identity_budget`], the objects start in the world's own look
        /// and take the one asked for when they arrive ([`Self::offer_object_identity`]).
        ///
        /// # Errors
        /// As [`Self::load`].
        pub fn load_with_identity(
            store: &RetailDatStore,
            gpu: &mut Gpu,
            cfg: SceneConfig,
            identity: Option<Arc<dereth_client_runtime::object_identity::ObjectIdentity>>,
        ) -> Result<(Self, WorldState), WorldError> {
            // The world's own region: the world's hardware region where it has one, as a client of
            // the world's time drawing with 3D hardware loaded it.
            let region = world_region(store)?;
            // The ground and the sky: the world's own, or another era's (`[Render] Ground` and
            // `[Render] Sky`). A style whose files are not here leaves the world's own.
            let choice = match ground_for(store, &region, cfg.render.ground) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("the ground style is not drawn: {e:?}; the world's own is");
                    ground_for(store, &region, None).map_err(|e| match e {
                        StyleError::World(w) => w,
                        StyleError::Missing(_) => WorldError::MissingTerrainTexture,
                    })?
                }
            };
            let ground = choice.ground.map(Box::new);
            let ground_store = choice.files;
            let drawn = choice.drawn;
            let (sky_region, sky_store) =
                sky_for(store, &region, cfg.render.sky).unwrap_or_else(|e| {
                    tracing::warn!("the sky style is not drawn: {e:?}; the world's own is");
                    (None, None)
                });
            let (tex_merge, pal_shift) = land_surface(ground.as_deref().unwrap_or(&region))?;
            let pal_shifted = pal_shift.is_some();
            let splat = cfg.terrain_splat && gpu.terrain_splat_supported() && !pal_shifted;
            let table = height_table(&region);
            // The landscape surface cache, and `Render.LandscapeTextureDetail`'s
            // shift. Both live for the session, not for one `load`: the scene builds blocks as
            // the viewer walks and two neighbouring blocks share most of their merge keys.
            let mut merge = TerrainMergeCache::new();
            merge.shift = land_texture_scale_shift(cfg.render.landscape_texture_detail);
            let bake = BakeCache {
                surface_translucency: cfg.surface_translucency,
                // `Render.EnvironmentTextureDetail`, the same
                // `max(v, 1) - 1` the poll writes into.
                image_scale: dereth_render::texture::image_scale_from_shift(
                    crate::render_prefs::RenderPreferences::image_scale(
                        cfg.render.environment_texture_detail,
                    ),
                ),
                environment_texture_detail: cfg.render.environment_texture_detail,
                ..BakeCache::default()
            };
            // The objects' look: the world's own, or another era's (`[Render] Objects`). A style
            // whose files are not here leaves the world's own, and so, until they arrive, does
            // one whose verdicts the application is still working out.
            let mut identity = identity;
            let mut objects_waiting = false;
            let objects = match (
                object_files_for(store, cfg.render.objects),
                cfg.render.objects,
            ) {
                (Ok(Some(files)), Some(_)) => {
                    let interiors = store.interior_files(files.era());
                    if identity.is_none() && !cfg.object_identity_budget.is_some() {
                        identity = Some(object_identity(
                            store,
                            &files,
                            interiors.as_ref(),
                            cfg.object_identity_cache,
                        ));
                    }
                    if let Some(id) = &identity {
                        Some(ObjectLook::new(files, interiors, Arc::clone(id), &bake))
                    } else {
                        tracing::info!(
                            "the objects are drawn as the world's own until the other era's \
                             look is ready"
                        );
                        objects_waiting = true;
                        None
                    }
                }
                (Ok(_), _) => None,
                (Err(files), _) => {
                    tracing::warn!(
                        "the object mode is not drawn: {}; the world's own is",
                        files.objects_notice()
                    );
                    None
                }
            };
            let land = LandContext {
                region: Box::new(region),
                ground,
                ground_store,
                drawn,
                tex_merge,
                gpu_merge: cfg.gpu_terrain_merge && !pal_shifted,
                pal_shift,
                table: Box::new(table),
                lighting: LandscapeLighting::default(),
                merge,
                sources: HashMap::new(),
                gpu_merge_sources: HashMap::new(),
                mip_worker: crate::mip_worker::MipWorker::new(),
                defer_mips: cfg.stream_budget.is_some(),
                splat_supported: gpu.terrain_splat_supported() && !pal_shifted,
                splat_wanted: splat,
                composites_wanted: !splat,
                splat_sources: HashMap::new(),
                splats: HashMap::new(),
                bake,
                identity,
                objects,
                cells: dereth_client_runtime::env_cells::EnvCellLoader::new(),
            };

            // The terrain pipeline state. Row 1 of the catalogue -- an opaque textured surface --
            // with the two terrain-specific overrides land polygon drawing makes:
            // fixed-function lighting off and no rasteriser culling, because the landscape polygon draw has
            // already back-face-culled every triangle against the viewpoint on the CPU.
            let surface = RenderState {
                r#type: dereth_render::surface::surface_type::BASE1_IMAGE,
                handler: SurfaceHandler::Database,
                ..RenderState::default()
            };
            let ctx = SurfaceContext {
                vertex_format: VertexFormat::XyzDiffuseTex1,
                texture_is_set: true,
                lighting: false,
                ..SurfaceContext::default()
            };
            let (mut terrain_key, _) = PipelineKey::from_surface(&surface, ctx);
            terrain_key.cull = Cull::None;
            terrain_key.z_func = ZFunc::Less;
            terrain_key.z_write = true;
            let terrain_detail_key = PipelineKey::detail_second_pass(
                terrain_key,
                dereth_render::pso::DetailContent::Landscape,
            );

            // The world as the scene starts it, clock first: the landscape's own vertex
            // lighting is baked from the sky's light at the current time of day and every block
            // generated below reads it.
            let mut ws = WorldState::new(&land.region, &cfg);
            let mut scene = Self {
                cfg,
                // Leaves every shadow at the value
                // the first preference-update poll installs, so the frame after a load does
                // no work: seeded from the profile that was just applied, not from the defaults.
                render_shadow: cfg.render,
                // Region installation applies the detail-texturing preference to
                // both enabled detail classes; this scene's equivalent is the
                // `apply_detail_texturing` below, once `gpu` is reachable.
                detail: dereth_world_render::detail::DetailTexturing::new(),
                detail_textures: [None; 4],
                blocks: BTreeMap::new(),
                pending: BTreeMap::new(),
                awaiting: BTreeMap::new(),
                released_blocks: Vec::new(),
                land,
                terrain_key,
                terrain_detail_key,
                light_pools: LightPools::new(0.0),
                static_pool_key: None,
                character_parts: Vec::new(),
                character_part_levels: Vec::new(),
                character_part_draw_pos: Vec::new(),
                character_part_cypt: Vec::new(),
                force_level: -1,
                character_built_from: Vec::new(),
                object_draws: BTreeMap::new(),
                viewcone_check_object_id: std::cell::Cell::new(0),
                selected_part_drawn: std::cell::Cell::new(false),
                object_meshes: BTreeMap::new(),
                game_viewport: None,
                particle_gfx: ParticleGeometry::default(),
                frame_particles: std::cell::Cell::new(ParticleStats::default()),
                frame_material_parts: std::cell::Cell::new((0, 0)),
                frame_alpha_lists: std::cell::Cell::new(AlphaListStats::default()),
                frame_landscape_alpha: std::cell::Cell::new(LandscapeAlphaStats::default()),
                frame_alpha_pending: std::cell::RefCell::new(Vec::new()),
                frame_multipass_pending: std::cell::RefCell::new(Vec::new()),
                frame_alpha_order: std::cell::RefCell::new(Vec::new()),
                frame_blend_order: std::cell::RefCell::new(Vec::new()),
                frame_object_cone: std::cell::Cell::new(ObjectConeStats::default()),
                frame_part_order: std::cell::RefCell::new(Vec::new()),
                frame_drawn_cells: std::cell::RefCell::new(None),
                frame_pick_candidates: std::cell::RefCell::new(None),
                frame_outside_views: std::cell::RefCell::new(Vec::new()),
                frame_outside_view_count: std::cell::Cell::new(None),
                frame_cell_views: std::cell::RefCell::new(BTreeMap::new()),
                frame_portal_stamps: std::cell::Cell::new((0, 0)),
                detail_from: DetailSource::World,
                sky: None,
                sky_region: sky_region.map(Box::new),
                sky_store,
                sky_cache: None,
                // The default device states' fog quartet, until the first tick writes
                // the region's: colour `0x00AAAAAA`, 400..2000, disabled.
                fog: dereth_render::camera::FogParams::default(),
                degrade: DegradeState::new(cfg.auto_degrades),
                // The palette-shift landscape has no splat form (`splat` already says so).
                terrain_splat: splat,
                stats: SceneStats::default(),
            };

            // A look still waiting for its verdicts is not drawn yet: the shadow says the world's,
            // so the preference poll takes the one asked for once they arrive.
            if objects_waiting {
                scene.render_shadow.objects = None;
            }

            // The region is installed, so the
            // detail surfaces are generated at the preference's current value before anything is
            // drawn. The detail-texturing reset is reached for the
            // same reason.
            scene.apply_detail_texturing(store, gpu);
            // The landscape light tick, run once before anything is meshed.
            scene.apply_lighting(&mut ws);
            // ...and its fog tail, for the same reason: the first frame must not be drawn with
            // the default device states' grey 400..2000 while the region says otherwise.
            scene.apply_fog(&mut ws);

            // The window, scrolled onto the viewer's block. Every slot comes back `Fetched`
            // because the array did not exist, which is `update_block`'s full-reload path.
            let actions = ws.streamer.window.update_block(block_xy(cfg.landblock));
            scene.queue(&mut ws, &actions);
            // With a budget, one call may stop before any block exists (the viewer's own block
            // can be one the dat does not carry); keep going until one does or nothing is left.
            scene.stream(&mut ws, store, gpu)?;
            while scene.blocks.is_empty() && !scene.pending.is_empty() {
                scene.stream(&mut ws, store, gpu)?;
            }
            if scene.blocks.is_empty() {
                return Err(WorldError::NoSuchLandblock(cfg.landblock));
            }

            // The camera: the middle of the viewer's block, high enough to see it, looking north
            // and down. Fixed, so the headless capture is a regression test rather than a
            // screenshot.
            world_build::place_load_camera(&mut ws, &cfg)?;
            // **Deliberately un-recentred, and the reason is that a re-centre could
            // not change the answer.** This is one of the two arms that reach
            // [`Self::update_viewer_cell`] with no [`Self::recenter`] before it (the other is
            // [`Self::follow_character_now`]), which raises the question of whether it should
            // match the client's per-frame recenter step. It should not, on three
            // counts, and the first is the one that settles it:
            //
            // 1. **The index is block-independent, so the order does not matter.**
            //    [`Self::viewpoint_cell_id`] resolves the viewpoint to a *global* cell id --
            //    by computing `block*8 + floor(origin/24)`
            //    -- and `recenter` adds `(dx, dy)` to the block while subtracting `(dx, dy) * 192`
            //    from the origin, which is `+dx*8 - dx*8` in that sum. The global coordinate, and
            //    therefore `& 7` of it, is **invariant** under the re-centre. The load-time
            //    viewpoint test measures that over 1,089 stations rather than
            //    arguing it, including the exact 24 m and 192 m lines where a float shift could
            //    have flipped a `floor`. It holds only because the index is not clamped per block;
            //    a block-dependent clamp would give the same question a different answer.
            // 2. **A re-centre here would scroll the window off the block `load` was asked for.**
            //    The camera below is placed at `y = -0.35 * BLOCK_LENGTH` -- deliberately south of
            //    the block, so the whole of it is in frame -- and `floor(-67.2 / 192)` is `-1`. So
            //    `recenter` on this line would choose `(bx, by - 1)`, discarding the centre block
            //    this function has just streamed and just checked `NoSuchLandblock` against. That
            //    is a behaviour change to the **load**, not an ordering fix.
            // 3. **There is no retail counterpart to match.** The no-blit draw's single step runs on a
            //    *viewer* sphere path's current position, and this state -- the
            //    `--no-character` flycam -- has no player body
            //    and so no viewer at all. The client's behaviour with no
            //    viewpoint is normal-mode rendering's null-viewer arm, which touches nothing.
            //
            // The frame loop's own order is a **separate** question, answered and pinned at
            // [`Self::viewpoint`]; nothing here re-opens it.
            ws.update_viewer_cell();

            // Every gfx id any day group can name, built once and before the first
            // frame, for the same reason the terrain's textures are: `Gpu::upload_texture` runs a
            // command list of its own.
            scene.load_sky(store, gpu)?;
            // dt 0 at load: the first real frame advances the UV totals. A non-zero value here
            // would scroll the sky by a frame's worth before anything had been drawn.
            scene.update_sky(&mut ws, 0.0);
            scene.refresh_land_stats(&mut ws);
            Ok((scene, ws))
        }

        /// Update landscape lighting for the current time of day.
        ///
        /// ```text
        /// if (t, &ambLevel, &ambColor, &sunDir, &sunColor):
        ///     ambLevel = max(ambLevel, minimum_ambient)
        ///     (ambLevel, ambColor, sunDir, sunColor)
        /// ```
        ///
        /// **The sun direction is not normalised.** Its *length is the brightness*, and the
        /// landscape lighting's `n . sunlight`
        /// already carries the brightness. It is passed through untouched; normalising it flattens
        /// the whole day cycle.
        ///
        /// Returns true when the lighting changed, which is what makes the resident blocks re-bake:
        /// the landscape-lighting update ends by re-lighting every loaded landblock, and
        /// this crate reaches that through the window's own rebuild path.
        ///
        /// **One deliberate departure, and it is not observable.** The client re-lights every
        /// loaded block on *every* light tick without comparing; this compares first and skips the
        /// rebuild when the four values are bit-identical, which they are inside a flat stretch of
        /// the sky time-of-day ramp. `calc_lighting` is a pure function of the mesh and these four
        /// values, so the skipped rebuild would have produced the same vertices — and re-meshing a
        /// 49-block window to write the same bytes is a visible hitch for no pixels.
        fn apply_lighting(&mut self, ws: &mut WorldState) -> bool {
            // The landscape lighting override has two arms. With always-daylight
            // set the function **discards** the caller's four arguments and re-asks the region
            // for the lighting at `0.5f`; with it clear it takes
            // what the time update computed for `present_time_of_day`. The caller clamps before
            // the override; only the always-daylight re-fetch repeats that clamp.
            // See
            // [`Self::set_always_daylight`] for why this one number is the whole option.
            let t = if ws.always_daylight {
                0.5
            } else {
                ws.clock.present_time_of_day
            };
            let Some(group) = dereth_world_render::sky::present_day_group(
                self.sky_region.as_deref().unwrap_or(&self.land.region),
                ws.clock.current_year,
                ws.clock.current_day,
            ) else {
                return false;
            };
            let l = dereth_world_render::sky::get_lighting(group, t);
            let mut want = LandscapeLighting {
                // Landscape minimum ambient is `0.2`, read from its static initializer.
                ambient_level: l.ambient_level.max(MIN_AMBIENT),
                ambient_color: l.ambient_color,
                sunlight: l.sun_vec,
                sunlight_color: l.sun_color,
            };
            let override_state = ws.environment_override.snapshot();
            if override_state.enabled {
                let transition = override_state.transition;
                // The per-frame update advances this blend before the landscape-lighting update.
                // Its always-daylight arm then discards
                // the blended ambient and re-asks the region for noon, but the shared transition
                // has still advanced. This ordering is why the increment is outside the guard.
                if !ws.always_daylight {
                    if transition >= 1.0 {
                        want.ambient_level = override_state.ambient_level;
                        want.ambient_color = color_rgb(override_state.ambient_color);
                    } else {
                        want.ambient_level = blend_override_scalar(
                            want.ambient_level,
                            override_state.ambient_level,
                            transition,
                        );
                        let base = (u32::from(want.ambient_color[0]) << 16)
                            | (u32::from(want.ambient_color[1]) << 8)
                            | u32::from(want.ambient_color[2]);
                        want.ambient_color = color_rgb(blend_override_color(
                            base,
                            override_state.ambient_color,
                            transition,
                        ));
                    }
                }
                if transition < 1.0 {
                    ws.environment_override.advance();
                }
            }
            if want == self.land.lighting {
                return false;
            }
            self.land.lighting = want;
            true
        }

        /// The landscape time update's fog tail:
        ///
        /// ```text
        /// mark fog user-disabled unless the fog option is on
        /// if user/world fog is disabled, return
        /// enable fixed-function fog
        /// if the world has no fog values at time t, return
        /// if the admin override is enabled, blend toward it
        /// write the fog color, minimum and maximum states
        /// ```
        ///
        /// The fog-state update writes `D3DRS_FOGCOLOR` (0x22, packed
        /// ARGB), `D3DRS_FOGSTART` (0x24) from the minimum, and `D3DRS_FOGEND` (0x25) from the maximum.
        /// The mode was fixed once at start-up and never changes: table `D3DFOG_NONE`,
        /// vertex `D3DFOG_LINEAR`, `D3DRS_RANGEFOGENABLE` 1 — see
        /// [`dereth_world_render::sky::linear_fog_factor`].
        ///
        /// Three things this does **not** do, each read rather than assumed:
        ///
        /// * **No draw-distance preference enters the range.** `Render.LandscapeDrawDistance`
        ///   does not scale it; the fog update reads only
        ///   the world's minimum and maximum fog distances.
        /// * **Indoors is not special.** System fog disablement is controlled only by the map
        ///   camera's two mode transitions. An environment cell carries no fog field and no cell turns
        ///   fog off; an interior is
        ///   drawn with the world's fog and simply has nothing far enough away to show it.
        /// * **Admin override only.** The visual environment command is the sole writer of the
        ///   override state; this reproduces that writer.
        ///
        /// Returns true when the fog moved, for symmetry with [`Self::apply_lighting`]; nothing
        /// has to be rebuilt when it does, because the fog is a per-frame constant and not a bake.
        fn apply_fog(&mut self, ws: &mut WorldState) -> bool {
            let (want, advance_override) = self.computed_world_fog_state(ws);
            if advance_override {
                ws.environment_override.advance();
            }
            if want == self.fog {
                return false;
            }
            self.fog = want;
            true
        }

        /// The [`dereth_render::camera::FogParams`] the region asks for at the clock's current
        /// moment, before [`Self::apply_fog`] latches it.
        ///
        /// [`dereth_world_render::sky::get_world_fog`] answering `None` is "leave the fog alone", which on the device means the
        /// previous colour and range stay installed with `D3DRS_FOGENABLE` still on. It cannot
        /// happen in the shipped region — every sky time-of-day row of all twenty day groups has
        /// `world_fog = 1` — so the arm is reproduced as "keep what is installed" rather than
        /// being given an invented default.
        #[must_use]
        pub fn world_fog_state(&self, ws: &WorldState) -> dereth_render::camera::FogParams {
            self.computed_world_fog_state(ws).0
        }

        fn computed_world_fog_state(
            &self,
            ws: &WorldState,
        ) -> (dereth_render::camera::FogParams, bool) {
            if !self.cfg.world_fog {
                return (
                    dereth_render::camera::FogParams {
                        enabled: false,
                        ..self.fog
                    },
                    false,
                );
            }
            let Some(group) = dereth_world_render::sky::present_day_group(
                self.sky_region.as_deref().unwrap_or(&self.land.region),
                ws.clock.current_year,
                ws.clock.current_day,
            ) else {
                return (
                    dereth_render::camera::FogParams {
                        enabled: true,
                        ..self.fog
                    },
                    false,
                );
            };
            let Some(f) =
                dereth_world_render::sky::get_world_fog(group, ws.clock.present_time_of_day)
            else {
                return (
                    dereth_render::camera::FogParams {
                        enabled: true,
                        ..self.fog
                    },
                    false,
                );
            };
            let mut want = dereth_render::camera::FogParams {
                // The fog state's packed ARGB, with alpha 0xFF as the world-fog query forces it.
                color: 0xFF00_0000
                    | (u32::from(f.color[0]) << 16)
                    | (u32::from(f.color[1]) << 8)
                    | u32::from(f.color[2]),
                near: f.min,
                far: f.max,
                enabled: true,
            };
            let mut advance_override = false;
            let override_state = ws.environment_override.snapshot();
            if override_state.enabled {
                let transition = override_state.transition;
                if transition >= 1.0 {
                    want.color = override_state.fog_color;
                    want.near = override_state.fog_min;
                    want.far = override_state.fog_max;
                } else {
                    want.color =
                        blend_override_color(want.color, override_state.fog_color, transition);
                    want.near =
                        blend_override_scalar(want.near, override_state.fog_min, transition);
                    want.far = blend_override_scalar(want.far, override_state.fog_max, transition);
                    advance_override = true;
                }
            }
            (want, advance_override)
        }

        /// The device fog state as it stands, for the tests and the log line.
        #[must_use]
        pub fn world_fog(&self) -> dereth_render::camera::FogParams {
            self.fog
        }

        /// The sky update and the two timers around it.
        ///
        /// The sky updates **every frame** — it is called before the `tick_size` gate — while the
        /// lighting and the fog are behind `next_light_tick` / `next_tick`.
        fn update_sky(&mut self, ws: &mut WorldState, dt: f32) {
            let (t, year, day) = (
                ws.clock.present_time_of_day,
                ws.clock.current_year,
                ws.clock.current_day,
            );
            if let Some(sky) = self.sky.as_mut() {
                // `dt` for the sky animation accumulator: the sky's UV scroll is `dt`-scaled and
                // needs the elapsed time to scale by.
                sky.use_time(
                    self.sky_region.as_deref().unwrap_or(&self.land.region),
                    year,
                    day,
                    t,
                    dt,
                );
                self.stats.sky_objects = sky.live();
                self.stats.sky_stats = sky.stats;
            }
        }

        /// Re-bake every resident block's vertex lighting, which is the landscape-lighting update's
        /// tail.
        ///
        /// The client calls on each loaded block directly; this
        /// crate reaches the same place by asking [`Self::stream`] to regenerate each slot's mesh,
        /// because `LandContext::generate` already runs `bake_lighting` after every geometry
        /// rebuild and a block's baked objects survive a `SlotWork::Mesh`.
        fn relight_blocks(&mut self, ws: &mut WorldState) {
            let w = ws.streamer.window.mid_width();
            for xi in 0..w {
                for yi in 0..w {
                    if ws
                        .streamer
                        .window
                        .slot(xi, yi)
                        .and_then(|s| s.mesh.as_ref())
                        .and_then(SlotMesh::get::<LandblockMesh>)
                        .is_none()
                    {
                        continue;
                    }
                    self.want(ws, xi, yi, SlotWork::Mesh);
                }
            }
        }

        /// The landscape window's bookkeeping — turn one window update into the
        /// work [`Self::stream`] has to do, release what left the window, and re-anchor what
        /// stayed.
        ///
        /// The re-anchor is the whole reason a scroll is cheap: the window
        /// gives every block an origin relative to the **viewer's** block, so a scroll changes
        /// every resident block's origin and nothing else. A block's vertices —
        /// terrain *and* objects — are block-local, so that is one `(f32, f32)` per block rather
        /// than a re-transform of everything in the window.
        fn queue(&mut self, ws: &mut WorldState, actions: &[SlotAction]) {
            let w = ws.streamer.window.mid_width();
            let mut live: BTreeSet<(i32, i32)> = BTreeSet::new();
            for (index, action) in actions.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: index arithmetic over the window, whose width is at most 31. Not a
                // float conversion.
                let index = index as u32;
                let (xi, yi) = (index / w, index % w);
                let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                    continue;
                };
                let block = (slot.block_x, slot.block_y);
                live.insert(block);
                match action {
                    SlotAction::Fetched { .. } => self.want(ws, xi, yi, SlotWork::Full),
                    // A size change then `generate`, or the `rebuilt = 0` tail: the mesh
                    // and its per-cell surfaces are rebuilt, the block's objects are not.
                    SlotAction::Resized { .. } | SlotAction::Restitched { .. } => {
                        self.want(ws, xi, yi, SlotWork::Mesh);
                    }
                    SlotAction::Unchanged | SlotAction::Released => {}
                }
                let origin = ws.streamer.window.block_frame_origin(xi, yi);
                let baked = self.blocks.get(&block).is_some_and(|b| b.baked);
                let resident = self.blocks.contains_key(&block);
                if let Some(b) = self.blocks.get_mut(&block) {
                    b.origin = origin;
                }
                // A block that scrolled **into** the scenery radius has to grow its objects now,
                // and one that scrolled out of it has to release them
                // Neither is an `update_block`
                // action, because the client's scenery radius is its whole window.
                if resident && baked != self.wants_objects(ws, xi, yi) {
                    self.want(ws, xi, yi, SlotWork::Full);
                }
            }
            // The client releases every block the scroll pushed off the
            // edge — it calls the release from four sites, one per edge.
            //
            // **This is the whole of the block release, not its first line.** The
            // client's three lines release objects, release the block's visible cells,
            // and release the block; this `retain` is the first and
            // the third (a `BlockDraw` owns its meshes, batches and degrade table outright, so
            // dropping it is both), and [`Self::release_block_interiors`] is the second. Without
            // the second line a walk keeps every interior it has ever
            // entered solid in `PhysicsWorld` and its furniture in the cell tables for the life of
            // the session. `CellStaticObjects::release_block` must not run on its own: destroying
            // the furniture while the walls stayed solid would be a load/unload heuristic of this
            // build's own invention; releasing the walls in the same breath is what removes that
            // objection.
            let departed: Vec<(i32, i32)> = self
                .blocks
                .keys()
                .copied()
                .filter(|k| !live.contains(k))
                .collect();
            // The `BlockDraw` is *moved* out rather than dropped, because
            // releasing the graphics-object arrays is part of this teardown too: every surface group
            // the block's bake resolved took a link out of [`BakeCache`], and dropping the batches
            // returned none of them. That was the last unbounded growth in the renderer — the
            // non-appearance half of `BakeCache::surfaces` went ~490 -> 964 slots over a 1,098.5 s
            // `long-solo-play` replay, about 0.43 slots per second of play, because a block that left
            // the window took its links with it. Settled in [`Self::stream`], which has the
            // device; see [`Self::released_blocks`].
            for k in &departed {
                if let Some(b) = self.blocks.remove(k) {
                    self.released_blocks.push(b);
                }
            }
            debug_assert!(self.blocks.keys().all(|k| live.contains(k)));
            self.release_block_interiors(ws, &departed);
        }

        /// Release the cells in the block's visible-cell list and their static objects:
        /// the middle step of whole-block release.
        ///
        /// It is the exact mirror of what [`Self::stream`] does on the way in: `stream` calls
        /// `DatLandSource::load_block_cells` and then
        /// [`Self::init_cell_statics`], and this
        /// undoes the second and then the first, in that order, because a body may not outlive the
        /// cell it stands in.
        ///
        /// **What is released and what survives.** Released: the block's `EnvCellGeometry`
        /// entries (the walls physics collides against), its sorted-cell building shells and their
        /// portal wiring, its prefetch mark, and every physics body its cells baked in. Survives:
        /// every decode memo — `EnvCellLoader::environments` and `CellStaticObjects::geometry` —
        /// which is the client's shared asset cache and is not a cell's to free. That
        /// split is why a re-entered block re-registers rather than re-decoding.
        ///
        /// A block with no body has nothing to release from, which is `--no-character`'s flycam:
        /// the cells were never given to physics in the first place, because
        /// [`Self::init_cell_statics`] returns early without one.
        fn release_block_interiors(&mut self, ws: &mut WorldState, departed: &[(i32, i32)]) {
            let released = world_build::release_block_interiors(ws, &self.cfg, departed);
            self.stats.cell_statics_destroyed += released.cell_statics_destroyed;
            self.stats.cells_released += released.cells_released;
            self.stats.buildings_released += released.buildings_released;
            self.stats.blocks_released += released.blocks_released;
        }

        /// Release the merged terrain surfaces for one block: one reference released
        /// per cell, and the device slot handed back for
        /// every surface whose last cell that was.
        ///
        /// This is the terrain half of the departed-block release. The terrain's merged surfaces
        /// do not come from [`BakeCache`] — they come from `LandContext::merge`, so without this
        /// a block that left the streaming window would take its merged surfaces with it and
        /// nothing would return them. The [`BakeCache`] memo stays **flat** across such a walk,
        /// which is why the leak is invisible there: over a 24-station two-lap walk
        /// `BakeCache`'s residency reads 345 pairs at all three home stations while the device's
        /// own live count, without this release, goes from 560 at the load to 689 on the way home.
        ///
        /// **The reference is per cell, not per distinct key**, because that is where retail takes
        /// it: terrain selection's cache-hit arm increments the surface's cell count exactly as
        /// adding a new surface initialises it, and surface removal iterates `side_cell_count²` cells.
        /// A 9x9 block whose 64 cells all merge to one surface therefore holds 64 references on
        /// it, and gives all 64 back.
        fn release_block_terrain(&mut self, gpu: &mut Gpu, keys: &[MergeKey]) {
            for k in keys {
                self.stats.terrain_surface_releases += 1;
                if let Some(h) = self.land.merge.remove_surface(*k) {
                    if h.0 != NO_TEXTURE {
                        gpu.release_texture(TextureSlot(h.0));
                    }
                    self.stats.terrain_surfaces_freed += 1;
                }
            }
        }

        /// Every descriptor slot one baked block's geometry took a link on.
        ///
        /// The terrain's own `cell_texture` is deliberately **not** here: those handles come from
        /// `LandContext::merge`, which is the world-render crate's
        /// `MergeCache` and has its own link book and its own release edge —
        /// [`Self::release_block_terrain`] — keyed on the merge key rather than on
        /// a descriptor slot, because that is the unit terrain-surface removal counts in. Offering one of
        /// its handles to [`BakeCache::release_group_texture`] would only count an
        /// `unowned_release`.
        /// Every descriptor slot a bake took a link on, as [`Self::block_texture_slots`] for a bake
        /// that never became a block.
        fn baked_texture_slots(b: &BakedObjects) -> (TextureLinks, TextureLinks) {
            Self::slots_of(&b.opaque, &b.blended, &b.env_cells)
        }

        /// A block's slots in two lists, its exterior objects' and its interior cells', each with
        /// whether it took its link from the objects' look's cache.
        fn block_texture_slots(b: &BlockDraw) -> (TextureLinks, TextureLinks) {
            Self::slots_of(&b.opaque, &b.blended, &b.env_cells)
        }

        fn slots_of(
            opaque: &[StaticBatch],
            blended: &[StaticBatch],
            env_cells: &[EnvCellDraw],
        ) -> (TextureLinks, TextureLinks) {
            fn batches(v: &[StaticBatch]) -> impl Iterator<Item = (TextureSlot, bool)> + '_ {
                v.iter().filter_map(|s| s.texture.map(|t| (t, s.from_look)))
            }
            let outside: Vec<(TextureSlot, bool)> =
                batches(opaque).chain(batches(blended)).collect();
            let mut inside = Vec::new();
            for cell in env_cells {
                inside.extend(
                    cell.meshes
                        .iter()
                        .filter_map(|m| m.texture.map(|t| (t, cell.from_look))),
                );
                inside.extend(batches(&cell.statics));
                inside.extend(batches(&cell.statics_blended));
            }
            (outside, inside)
        }

        /// Return one link to the cache that handed it out: the objects' look's when `from_look`
        /// (and the look is drawn), the world's otherwise. Two caches can hold the same slot,
        /// each with its own link, so the owner is said rather than looked up.
        fn release_look_texture(
            &mut self,
            gpu: &mut Gpu,
            slot: TextureSlot,
            from_look: bool,
        ) -> bool {
            match self.land.objects.as_mut() {
                Some(look) if from_look => look.cache.release_group_texture(gpu, slot),
                _ => self.land.bake.release_group_texture(gpu, slot),
            }
        }

        /// Release **textures** for every block
        /// [`Self::queue`] pushed out of the window.
        ///
        /// One release per link taken, which is one per batch and one per cell mesh that carries a
        /// texture — the same one-for-one discipline
        /// [`Self::release_unlinked_appearances`] uses, and the reason
        /// [`BakeCache::unowned_releases`] can be asserted at zero.
        fn release_departed_blocks(&mut self, gpu: &mut Gpu) {
            if self.released_blocks.is_empty() {
                return;
            }
            for b in std::mem::take(&mut self.released_blocks) {
                let (outside, inside) = Self::block_texture_slots(&b);
                for (slot, from_look) in outside.into_iter().chain(inside) {
                    self.stats.block_texture_releases += 1;
                    if self.release_look_texture(gpu, slot, from_look) {
                        self.stats.block_textures_freed += 1;
                    }
                }
                // Block teardown removes the block's surfaces
                // before it frees the cells, so the merged terrain surfaces go back on the same
                // teardown as the object textures above.
                self.release_block_terrain(gpu, &b.cell_keys);
            }
        }

        /// Fetch and generate every block [`Self::queue`] asked for.
        ///
        /// **Called outside the frame bracket**, from the client shell's `App::frame`, for the same
        /// reason [`Self::sync_objects`] is: `Gpu::upload_texture` resets and closes a command
        /// list of its own. The client's equivalent drains asynchronous fetches from the shared asset
        /// cache in a separate frame phase as well.
        ///
        /// # Errors
        /// [`WorldError::Render`] when a device resource cannot be created.
        pub fn stream(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
        ) -> Result<(), WorldError> {
            // **Before the early return, deliberately.** A frame can have blocks to
            // release and none to build (the outermost ring leaving a window that is shrinking, or
            // a scroll into blocks the dat does not carry), and gating the release on the arrival
            // queue would make the leak come back for exactly those frames.
            self.release_departed_blocks(gpu);
            self.land.mip_worker.poll();
            self.convert_terrain(ws, store, gpu)?;
            if self.pending.is_empty() {
                return Ok(());
            }
            let _stream =
                tracing::debug_span!("stream_landblocks", queued = self.pending.len()).entered();
            let mut touched: BTreeSet<(i32, i32)> = BTreeSet::new();
            let started = web_time::Instant::now();
            let mut order: Vec<((i32, i32), SlotWork)> =
                self.pending.iter().map(|(&b, &w)| (b, w)).collect();
            if self.cfg.stream_budget.is_some() {
                // Nearest first, so the blocks the player stands in and looks at arrive before
                // the horizon does. Stable, so ties keep window order.
                let viewer = ws.streamer.window.viewer_block().unwrap_or((0, 0));
                order.sort_by_key(|&((bx, by), _)| {
                    let (dx, dy) = (bx - viewer.0, by - viewer.1);
                    (dx.abs().max(dy.abs()), dx * dx + dy * dy)
                });
            }
            for (block, work) in order {
                // At least one block per call, so a budget smaller than one block still makes
                // progress.
                if self
                    .cfg
                    .stream_budget
                    .is_some_and(|b| started.elapsed() >= b)
                    && !touched.is_empty()
                {
                    break;
                }
                // A block waiting on the mip worker stays queued until everything it asked for is done.
                if let Some(keys) = self.awaiting.get(&block) {
                    if !self.land.mip_worker.all_done(keys) {
                        continue;
                    }
                    self.awaiting.remove(&block);
                }
                self.pending.remove(&block);
                // A block that scrolled out of the window before it was built has nothing to
                // build into.
                let Some((xi, yi)) = self.slot_of_block(ws, block) else {
                    continue;
                };
                touched.insert(block);
                self.build_slot(ws, store, gpu, xi, yi, work)?;
            }
            // Load each touched block's interior cells on the **physics** side. It is
            // here rather than inside `env_cell` because `env_cell` is a pure decoder and
            // must never load (see [`dereth_client_runtime::land_source`]); "the same path that loads landblocks"
            // is this one.
            if let Some(land) = ws.character.as_ref().map(|c| Arc::clone(c.land())) {
                for (bx, by) in touched {
                    if !(0..=0xFE).contains(&bx) || !(0..=0xFE).contains(&by) {
                        continue;
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // LINT-OK: both bounded to 0..=0xFE above. Not a float conversion.
                    let id = dereth_primitives::LandblockId(((bx as u16) << 8) | (by as u16));
                    land.load_block_cells(id);
                }
            }
            self.init_cell_statics(ws, store);
            // Deliberately *outside* `init_cell_statics` and outside its
            // `SceneConfig::cell_statics` switch: a house barrier is not a static object, and
            // `--no-cell-statics` is the control for the solidity of the landscape, not for
            // who may walk onto a property.
            self.init_cell_restrictions(ws);
            // A block that has just been baked has no assembled batches yet, and
            // `stream` runs outside the frame bracket, so a frame that streams and draws without
            // an intervening `update` would draw nothing of it. This is the only reason the call
            // is here as well as in `update`. The switches it finds are counted like any other:
            // a counter that silently drops the ones a streaming frame makes would undercount the
            // exact frame in which the switches occur.
            let switches = self.refresh_degrade_levels(ws);
            self.stats.degrade_switches += u64::from(switches);
            self.refresh_land_stats(ws);
            Ok(())
        }

        /// **The per-frame rendering-preference poll.**
        ///
        /// Graphics-device preparation runs this on **every** frame.
        /// It is a poll and not a callback: the options page writes the render-preference fields
        /// and nothing else (the preference-change callback
        /// recomputes one cached quality level and applies nothing), so the whole apply
        /// is this function comparing each field against its shadow copy.
        ///
        /// The four fields this scene owns and their effects are:
        ///
        /// | field compared with its shadow | on a change |
        /// |---|---|
        /// | `LandscapeTextureDetail` | land texture scale = `max(v,1)-1`, then flush graphics resources |
        /// | `EnvironmentTextureDetail` | clip-map, RGBA and indexed texture scales = `max(v,1)-1`, then the same flush |
        /// | `LandscapeDrawDistance` | update the landscape middle radius |
        /// | environment detail textures | apply detail texturing with `(false, v)` |
        ///
        /// `Render.MultiPassAlpha` is deliberately **not** here and has no shadow in retail
        /// either: the landscape draw reads that preference live, per mesh,
        /// per frame. `WorldScene::draw_part` does the same, so it needs no poll.
        ///
        /// # Graphics-resource flushing, and what it is here
        ///
        /// Graphics-resource flushing unbinds all eight texture stages and purges resources
        /// (every live graphics resource is asked to destroy its device object),
        /// then restores lost resources, so the next build re-creates every texture at the new
        /// texture-detail scale. The equivalent here is [`Self::flush_graphics_resources`]
        /// followed by a window rebuild: every resident landblock hands its merged terrain
        /// surfaces and its object textures back — which takes their descriptor pairs out of the
        /// device's `texture_table`, so the rebuild cannot hit a stale cached upload — and is then
        /// generated again at the new shift and the new scale.
        ///
        /// # Detail-texturing preference
        ///
        /// `Render.BuildingDetailTextures` (the options page's **Environment Detail Textures**) is
        /// polled and counted here, and a change
        /// calls [`Self::apply_detail_texturing`], matching retail's `(false, v)` application and
        /// landscape detail-texturing setup `(false, v, v, 0)`, which runs
        /// detail-surface cleanup, detail-surface generation,
        /// then updates the four surface/tiling pairs. Mesh drawing reads the current
        /// detail surface, which suppresses alpha-list deferral entirely, and hands it
        /// to subset rendering as a second texture stage. That is an
        /// implemented subsystem rather than a count-only switch. **Named, counted, and consumed.**
        ///
        /// # Errors
        /// [`WorldError`] when the rebuild cannot make a device resource.
        pub fn update_from_preferences(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
        ) -> Result<RenderPrefWork, WorldError> {
            let was = self.render_shadow;
            let mut work = RenderPrefWork::default();
            // This client's landscape options first. A style whose files are not here is refused
            // and the option goes back to the one drawn.
            let asked = self.cfg.render;
            if asked.ground != was.ground {
                match self.set_ground(ws, store, gpu, asked.ground)? {
                    Ok(()) => work.ground_changed = true,
                    Err(files) => {
                        self.cfg.render.ground = was.ground;
                        work.ground_refused = Some((files, was.ground));
                    }
                }
            }
            if asked.sky != was.sky {
                match self.set_sky(ws, store, gpu, asked.sky)? {
                    Ok(()) => work.sky_changed = true,
                    Err(files) => {
                        self.cfg.render.sky = was.sky;
                        work.sky_refused = Some((files, was.sky));
                    }
                }
            }
            if asked.objects != was.objects {
                if self.objects_wait_for_identity(store, asked.objects) {
                    // The look is drawn once the verdicts arrive; until then the objects keep
                    // the one they have, and the poll asks again next frame.
                    work.objects_waiting = true;
                } else {
                    match self.set_objects(ws, store, gpu, asked.objects)? {
                        Ok(()) => work.objects_changed = true,
                        Err(files) => {
                            self.cfg.render.objects = was.objects;
                            work.objects_refused = Some((files, was.objects));
                        }
                    }
                }
            }
            if work.ground_changed || work.objects_changed {
                work.blocks_rebuilt = self.blocks.len();
            }
            let live = self.cfg.render;
            // `Render.AutomaticDegrades` turns the governor on or off from the next frame; off,
            // the detail bias is the manual one.
            if live.automatic_degrades != was.automatic_degrades {
                self.degrade = DegradeState::new(live.automatic_degrades);
            }
            // Either texture-detail level raises the **same** flag, and that flag is what reaches the
            // graphics-resource flush.
            let land_detail = live.landscape_texture_detail != was.landscape_texture_detail;
            let env_detail = live.environment_texture_detail != was.environment_texture_detail;
            work.flushed = land_detail || env_detail;
            // The landscape draw-distance comparison, which reaches `set_mid_radius`.
            work.mid_radius_changed = live.landscape_draw_distance != was.landscape_draw_distance;
            // The environment-detail-textures compare against its shadow, and the
            // landscape-detail-textures one, which the clients before the end-of-retail one
            // made too.
            work.detail_texturing_changed = live.environment_detail_textures
                != was.environment_detail_textures
                || live.landscape_detail_textures != was.landscape_detail_textures;
            self.render_shadow = live;
            if work.objects_waiting {
                self.render_shadow.objects = was.objects;
            }
            if work.detail_texturing_changed {
                // Call the detail-texturing setter with `(landscape, v, v, 0)`.
                // This arm is what makes the preference more than a counter.
                self.apply_detail_texturing(store, gpu);
                work.detail_surfaces = self.detail.generated();
            }
            if !(work.flushed || work.mid_radius_changed) {
                return Ok(work);
            }
            if work.flushed {
                // The landscape texture scale consumed by terrain texture generation.
                self.land.merge.shift = land_texture_scale_shift(live.landscape_texture_detail);
                // The environment texture scale shared by clip-map, RGBA, and indexed uploads;
                // the preference poll copies this into the current scale used by texture creation.
                self.land.bake.image_scale = dereth_render::texture::image_scale_from_shift(
                    crate::render_prefs::RenderPreferences::image_scale(
                        live.environment_texture_detail,
                    ),
                );
                self.flush_graphics_resources(ws, gpu);
            }
            // Changing the middle radius releases the whole
            // landblock array, sets the new radius, and then updates the viewpoint with the
            // repopulate flag set.
            // A fresh [`LandblockWindow`] is the first two -- its constructor is the only way to
            // set the radius, exactly because the client only lets the mid-radius setter succeed on a
            // released array -- and `update_block` is the third.
            let radius = if work.mid_radius_changed {
                self.cfg.land_radius = live.landscape_draw_distance;
                live.landscape_draw_distance
            } else {
                ws.streamer.window.mid_radius()
            };
            let viewer = ws.streamer.window.viewer_block();
            ws.streamer.window = LandblockWindow::new(radius);
            if let Some(v) = viewer {
                // Every slot comes back `Fetched` because the array did not exist, which is
                // `update_block`'s full-reload path -- the same one `WorldScene::load` takes.
                let actions = ws.streamer.window.update_block(v);
                self.queue(ws, &actions);
            }
            work.blocks_queued = self.pending.len();
            // The rebuild runs **now** rather than waiting for the frame's own `stream`, because
            // this is called from the frame step immediately before it and the next frame must
            // show the change.
            self.stream(ws, store, gpu)?;
            work.blocks_rebuilt = self.blocks.len();
            Ok(work)
        }

        /// Build the sky from its region and files: the world's own through the scene's surface
        /// cache, another era's through a cache of its own ([`Self::sky_cache`]).
        fn load_sky(&mut self, store: &RetailDatStore, gpu: &mut Gpu) -> Result<(), WorldError> {
            let region = self.sky_region.as_deref().unwrap_or(&self.land.region);
            let sky = match self.sky_store.as_ref() {
                Some(files) => {
                    let cache = self.sky_cache.get_or_insert_with(|| BakeCache {
                        surface_translucency: self.land.bake.surface_translucency,
                        image_scale: self.land.bake.image_scale,
                        environment_texture_detail: self.land.bake.environment_texture_detail,
                        ..BakeCache::default()
                    });
                    crate::sky::SkyScene::load(files, region, gpu, cache)
                }
                None => crate::sky::SkyScene::load(store, region, gpu, &mut self.land.bake),
            }
            .map_err(|e| WorldError::Render(e.to_string()))?;
            self.sky = Some(sky);
            Ok(())
        }

        /// Release the window and build it again around the viewer at `radius`: every slot comes
        /// back `Fetched` because the array is new, which is the full-reload path the load takes.
        /// The rebuild runs now, within the streaming budget, so the next frame shows it.
        fn rebuild_window(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            radius: u32,
        ) -> Result<(), WorldError> {
            let viewer = ws.streamer.window.viewer_block();
            ws.streamer.window = LandblockWindow::new(radius);
            if let Some(v) = viewer {
                let actions = ws.streamer.window.update_block(v);
                self.queue(ws, &actions);
            }
            self.stream(ws, store, gpu)
        }

        /// `[Render] Ground`, live: draw the ground with `style`'s land surface (`None`: the
        /// world's own) from the next frame. Every merged surface, decoded source and splat the old
        /// ground made is released, the landscape detail texture is taken from the new ground, and
        /// the window is rebuilt around the viewer.
        ///
        /// `Ok(Err(files))` when the style's files are not present: nothing changes, and `files`
        /// says which were wanted.
        ///
        /// # Errors
        /// [`WorldError`] when the style's region will not decode or the rebuild cannot make a
        /// device resource.
        pub fn set_ground(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            style: Option<RegionStyle>,
        ) -> Result<Result<(), RequiredFiles>, WorldError> {
            let GroundChoice {
                ground,
                files: ground_store,
                drawn,
            } = match ground_for(store, &self.land.region, style) {
                Ok(g) => g,
                Err(StyleError::Missing(files)) => return Ok(Err(files)),
                Err(StyleError::World(e)) => return Err(e),
            };
            let (tex_merge, pal_shift) =
                land_surface(ground.as_ref().unwrap_or(&self.land.region))?;
            // Splatting is what the landscape was doing, or what it was asked to do before a
            // palette-shift ground (which has no splat form) turned it off.
            let wanted_splat = if self.land.pal_shift.is_some() {
                self.cfg.terrain_splat
            } else {
                self.terrain_splat
            };
            // The old ground's surfaces go first, so no surface of one technique can be handed to
            // a cell of the other under the same merge key.
            self.flush_graphics_resources(ws, gpu);
            for h in self.land.merge.drain() {
                if h.0 != NO_TEXTURE {
                    gpu.release_texture(TextureSlot(h.0));
                }
            }
            self.land.splats.clear();
            let pal_shifted = pal_shift.is_some();
            let splat_supported = gpu.terrain_splat_supported() && !pal_shifted;
            let splat = wanted_splat && splat_supported;
            self.land.ground = ground.map(Box::new);
            self.land.ground_store = ground_store;
            self.land.drawn = drawn;
            self.land.tex_merge = tex_merge;
            self.land.pal_shift = pal_shift;
            self.land.gpu_merge = self.cfg.gpu_terrain_merge && !pal_shifted;
            self.land.splat_supported = splat_supported;
            self.land.splat_wanted = splat;
            self.land.composites_wanted = !splat;
            self.terrain_splat = splat;
            self.cfg.render.ground = style;
            // The landscape detail texture is the ground's.
            self.apply_detail_texturing(store, gpu);
            let radius = ws.streamer.window.mid_radius();
            self.rebuild_window(ws, store, gpu, radius)?;
            self.refresh_land_stats(ws);
            tracing::info!(
                "the ground is drawn {} ({})",
                style.map_or("as the world's own", RegionStyle::ground_label),
                if self.land.pal_shift.is_some() {
                    "palette shift"
                } else {
                    "texture merge"
                }
            );
            Ok(Ok(()))
        }

        /// The object identity verdicts for this store, worked out by the application
        /// ([`dereth_client_runtime::object_identity::IdentityBuild`]): kept for every later
        /// switch of `[Render] Objects`, and a look asked for while they were not here is drawn
        /// at the next preference poll.
        pub fn offer_object_identity(
            &mut self,
            identity: Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
        ) {
            self.land.identity = Some(identity);
        }

        /// Whether drawing the objects with `style`'s look has to wait for the verdicts: the
        /// application works them out ([`SceneConfig::object_identity_budget`]), they are not here
        /// yet, and the look needs them (another era's files are drawn).
        fn objects_wait_for_identity(
            &self,
            store: &RetailDatStore,
            style: Option<RegionStyle>,
        ) -> bool {
            self.cfg.object_identity_budget.is_some()
                && self.land.identity.is_none()
                && style.is_some()
                && matches!(object_files_for(store, style), Ok(Some(_)))
        }

        /// `[Render] Objects`, live: draw the world's objects with `style`'s look (`None`: the
        /// world's own) from the next frame. Every block is released and baked again (its scenery,
        /// buildings and statics take the new look), every server object's appearance and the
        /// body are built again, and the old look's pictures go with the old meshes.
        ///
        /// `Ok(Err(files))` when the style's files are not present: nothing changes, and `files`
        /// says which were wanted.
        ///
        /// # Errors
        /// [`WorldError`] when a rebuild cannot make a device resource.
        pub fn set_objects(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            style: Option<RegionStyle>,
        ) -> Result<Result<(), RequiredFiles>, WorldError> {
            let files = match object_files_for(store, style) {
                Ok(f) => f,
                Err(files) => return Ok(Err(files)),
            };
            // Every block first, through the look that baked it.
            self.flush_graphics_resources(ws, gpu);
            // The old appearances and the body are kept until the new ones are built, so a
            // picture the two looks share is never taken to zero and uploaded again.
            let old_meshes = std::mem::take(&mut self.object_meshes);
            let old_body = std::mem::take(&mut self.character_parts);
            let new_look = match (files, style) {
                (Some(files), Some(_)) => {
                    let interiors = store.interior_files(files.era());
                    // A store has one other era beside it, so its verdicts are worked out once.
                    let identity = match &self.land.identity {
                        Some(i) => Arc::clone(i),
                        None => object_identity(
                            store,
                            &files,
                            interiors.as_ref(),
                            self.cfg.object_identity_cache,
                        ),
                    };
                    Some(ObjectLook::new(files, interiors, identity, &self.land.bake))
                }
                _ => None,
            };
            if let Some(look) = &new_look {
                self.land.identity = Some(Arc::clone(&look.identity));
            }
            let old_look = std::mem::replace(&mut self.land.objects, new_look);
            self.cfg.render.objects = style;
            // Every server object, built again; objects that shared an appearance share it again.
            let ids: Vec<ObjectId> = self.object_draws.keys().copied().collect();
            for id in ids {
                let Some(array) = ws
                    .objects
                    .get(&id)
                    .map(|o| o.sim.driver.borrow().part_array.parts.clone())
                else {
                    continue;
                };
                let key = self
                    .object_draws
                    .get(&id)
                    .and_then(|o| o.appearance.clone());
                let setup = self.object_draws.get(&id).map(|o| o.setup);
                let shared = key
                    .as_ref()
                    .and_then(|k| self.object_meshes.get(k))
                    .map(Arc::clone);
                let meshes = if let Some(m) = shared {
                    m
                } else {
                    let m = self.build_object_meshes(store, gpu, setup, &array, key.is_none())?;
                    let m = Arc::new(m);
                    if let Some(k) = key {
                        self.stats.appearance_builds += 1;
                        self.object_meshes.insert(k, Arc::clone(&m));
                    }
                    m
                };
                if let Some(o) = self.object_draws.get_mut(&id) {
                    o.meshes = meshes;
                }
            }
            // The body.
            if let Some((array, setup)) = ws
                .character
                .as_ref()
                .map(|c| (c.driver().part_array.parts.clone(), c.setup_id()))
            {
                let parts = self.build_object_meshes(store, gpu, Some(setup), &array, true)?;
                self.set_character_parts(gpu, parts);
            }
            // The old meshes give their links back: the world's parts through the world's
            // cache, the old look's parts all at once with its cache, which nothing else holds
            // now.
            for part in old_meshes.values().flat_map(|m| m.iter()).chain(&old_body) {
                if part.from_look {
                    continue;
                }
                for mesh in part.all() {
                    if let Some(slot) = mesh.texture {
                        self.land.bake.release_group_texture(gpu, slot);
                    }
                }
            }
            if let Some(mut look) = old_look {
                release_bake_cache(&mut look.cache, gpu);
            }
            let radius = ws.streamer.window.mid_radius();
            self.rebuild_window(ws, store, gpu, radius)?;
            self.stats.server_object_setups = self.object_meshes.len();
            self.stats.upload_bytes = self.worst_case_upload_bytes();
            self.refresh_surface_stats();
            self.refresh_land_stats(ws);
            tracing::info!(
                "the objects are drawn {}",
                match style {
                    None => "as the world's own",
                    Some(RegionStyle::Modern) => "with the later files' look",
                    Some(_) => "with the older files' look",
                }
            );
            Ok(Ok(()))
        }

        /// `[Render] Sky`, live: draw `style`'s sky (`None`: the world's own), with its light and
        /// fog, from the next frame. The old sky's objects go, and their textures with them when
        /// they were another era's; the landscape is re-lit when the light changed.
        ///
        /// `Ok(Err(files))` when the style's files are not present: nothing changes.
        ///
        /// # Errors
        /// [`WorldError`] when the style's region will not decode or a sky texture will not upload.
        pub fn set_sky(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            style: Option<RegionStyle>,
        ) -> Result<Result<(), RequiredFiles>, WorldError> {
            let (region, files) = match sky_for(store, &self.land.region, style) {
                Ok(s) => s,
                Err(StyleError::Missing(files)) => return Ok(Err(files)),
                Err(StyleError::World(e)) => return Err(e),
            };
            let (weather, override_enabled, fog_enabled) =
                self.sky.as_ref().map_or((true, false, false), |s| {
                    (s.weather_enabled, s.override_enabled, s.fog_enabled)
                });
            self.sky = None;
            if let Some(mut cache) = self.sky_cache.take() {
                release_bake_cache(&mut cache, gpu);
            }
            self.sky_region = region.map(Box::new);
            self.sky_store = files;
            self.cfg.render.sky = style;
            self.load_sky(store, gpu)?;
            if let Some(sky) = self.sky.as_mut() {
                sky.weather_enabled = weather;
                sky.override_enabled = override_enabled;
                sky.fog_enabled = fog_enabled;
            }
            if self.apply_lighting(ws) {
                self.relight_blocks(ws);
            }
            self.apply_fog(ws);
            self.update_sky(ws, 0.0);
            tracing::info!(
                "the sky is drawn {}",
                style.map_or("as the world's own", RegionStyle::sky_label)
            );
            Ok(Ok(()))
        }

        /// Flush graphics resources:
        /// throw every cached device texture away so the next build re-creates it at the current
        /// texture-detail scale.
        ///
        /// ```text
        ///   for (i = 0; i < 8; ++i) unbind texture stage i
        ///   purge graphics resources
        ///   restore lost resources
        ///   run each registered restore callback
        /// ```
        ///
        /// This build has no graphics-resource registry, and it does not need one: every texture
        /// a scene holds is reachable from a landblock's bake or its terrain cell keys, and both
        /// already have a correct release edge. So the flush is "release
        /// every resident block", and the caller re-queues them.
        ///
        /// **The order matters and is the reason this is a separate function.**
        /// [`Self::build_slot`] takes its *new* terrain references before it returns the old ones
        /// (deliberately, so a shared surface is never taken to zero and re-uploaded),
        /// which means a rebuild in place would hit the merge cache and keep the **old** shift.
        /// Releasing first is what makes the new shift reach the composite at all.
        ///
        /// **Declared departure.** Resource purging destroys every live graphics resource
        /// unconditionally; this returns links, so a texture a *server object* or the local body
        /// still holds keeps the scale it was uploaded at until that consumer is itself rebuilt.
        /// The landscape, the scenery, the buildings and the interior cells — which is everything
        /// `Render.EnvironmentTextureDetail` is named for — all go through a block and are covered.
        /// Building detail texturing passes `(landscape, v, v, 0)` to the landscape.
        ///
        /// The generation half is [`dereth_world_render::detail`], which has the whole chain at the
        /// bytes; this is the half that needs a dat store and a device — and
        /// the detail-surface construction inside the render path. A class
        /// whose `SurfaceTexture` will not decode keeps its surface and loses its texture, which
        /// is in effect the client releasing the custom surface when its texture is missing: the
        /// draw checks the texture, not the surface.
        ///
        /// **`object` is hard `false`**, as in every client. **`landscape` is
        /// `Render.LandscapeDetailTextures`**, as the clients before the end-of-retail one passed
        /// it (that one passes `0` and reads the preference nowhere); the preference is off by
        /// default, so a default profile draws what the end-of-retail client draws. The landscape
        /// class is read from the ground's region and its texture from the ground's files, which
        /// are the world's own unless an older world's ground is drawn with the later files.
        fn apply_detail_texturing(&mut self, store: &RetailDatStore, gpu: &mut Gpu) {
            use dereth_world_render::detail::DetailClass;
            // **The surfaces this is about to replace are released first, which is
            // the first thing the client does.**
            //
            // The client opens with a four-slot clear: for each content class it installs
            // an empty surface into the slot, and then
            //
            // releases the held custom surface if there is one and forgets it
            //
            // — four times, once per detail slot. Clearing `self.detail_textures` without releasing
            // would, on every re-application
            // — the detail-textures preference toggled, a region change — drop the client's
            // only handle on a slot whose link it still held, and the descriptor would never come
            // back: one 256x256 slot, the first texture of every scene, live after
            // `release_textures`.
            self.release_detail_textures(gpu);
            let on = self.cfg.render.environment_detail_textures;
            let landscape = self.cfg.render.landscape_detail_textures;
            let (detail_region, detail_files, from) = self.detail_source(store);
            self.detail_from = from;
            self.detail
                .set(Some(&detail_region), landscape, on, on, false);
            if !(on || landscape) {
                return;
            }
            for cls in [
                DetailClass::Landscape,
                DetailClass::Building,
                DetailClass::Environment,
            ] {
                let Some(s) = self.detail.surface(cls) else {
                    continue;
                };
                let files = detail_files.as_ref().unwrap_or(store);
                let textures = TextureStore::with_environment_texture_detail(
                    files,
                    self.cfg.render.environment_texture_detail,
                );
                // The combined-texture cache: the two live classes name
                // the *same* `SurfaceTexture` in the shipped region, so this is one upload and one
                // `AddRef`, not two images.
                let (key, _) = combined_key(&textures, Some(s.texture), None);
                let Ok(data) = textures.texture_data_shifted(s.texture, false, None) else {
                    continue;
                };
                // The detail texture is a `RenderSurface` like any other, and
                // texture creation shifts its extents by the same
                // current texture scale the object textures take.
                let Ok(data) =
                    dereth_render::texture::scale_surface(&data, self.land.bake.image_scale)
                else {
                    continue;
                };
                if let Ok(slot) = gpu.upload_imgtex_keyed(key, &data) {
                    self.detail_textures[cls.index()] = Some(slot);
                }
            }
        }

        /// The region the detail textures (all four classes) are read from, and its files
        /// (`None`: the world's): the drawn ground style's region when its land surface names
        /// detail textures (texture merging), else the world's own region when it does, else the
        /// end-of-retail files' region. The palette-shift land surface names none, so with Palette
        /// Shift the textures come from the world's own region.
        fn detail_source(
            &self,
            store: &RetailDatStore,
        ) -> (dereth_assets::Region, Option<RetailDatStore>, DetailSource) {
            if let Some(g) = self
                .land
                .ground
                .as_deref()
                .filter(|g| g.land_surf.tex_merge.is_some())
            {
                return (
                    g.clone(),
                    self.land.ground_store.clone(),
                    DetailSource::Ground,
                );
            }
            if self.land.region.land_surf.tex_merge.is_some() {
                return ((*self.land.region).clone(), None, DetailSource::World);
            }
            match style_region(store, RegionStyle::Modern) {
                Ok(s) if s.region.land_surf.tex_merge.is_some() => {
                    (s.region, s.files, DetailSource::EndOfRetail)
                }
                _ => ((*self.land.region).clone(), None, DetailSource::World),
            }
        }

        /// Where the detail textures were last taken from.
        #[must_use]
        pub fn detail_source_used(&self) -> DetailSource {
            self.detail_from
        }

        /// What detail-texturing setup's four-slot clear opens with:
        /// drop the link on every detail surface this scene holds and forget it. The sequence is
        /// described at [`Self::apply_detail_texturing`].
        ///
        /// The two live classes usually name the **same** slot — the combined-texture
        /// cache hands the second one an `AddRef` rather than a second image — so
        /// each entry releases the link it took, and the slot frees on the last of them.
        ///
        /// Returns the number of links handed back, so [`Self::release_textures`] can count them
        /// alongside the bake and terrain pairs.
        fn release_detail_textures(&mut self, gpu: &mut Gpu) -> u32 {
            let mut n = 0;
            // ORDER-OK: four independent slots, each released exactly once.
            for slot in self.detail_textures.iter_mut() {
                if let Some(s) = slot.take() {
                    gpu.release_texture(s);
                    n += 1;
                }
            }
            n
        }

        /// The current detail surface and detail tiling for one
        /// content class, as the draw that installs them needs them: the device texture and the
        /// tiling factor, or `None` when nothing is installed and the subset draw's
        /// want-detail flag is therefore false.
        fn current_detail(
            &self,
            cls: dereth_world_render::detail::DetailClass,
        ) -> Option<(TextureSlot, f32)> {
            let cur = self.detail.current(cls)?;
            Some((self.detail_textures[cls.index()]?, cur.tiling))
        }

        /// The detail state this scene holds: four current surfaces and four tiling factors, for
        /// the tests.
        #[must_use]
        pub fn detail_texturing(&self) -> dereth_world_render::detail::DetailTexturing {
            self.detail
        }

        fn flush_graphics_resources(&mut self, ws: &mut WorldState, gpu: &mut Gpu) {
            self.awaiting.clear();
            self.land.sources.clear();
            self.release_terrain_sources(gpu);
            let departed: Vec<(i32, i32)> = self.blocks.keys().copied().collect();
            for k in &departed {
                if let Some(b) = self.blocks.remove(k) {
                    self.released_blocks.push(b);
                }
            }
            self.release_block_interiors(ws, &departed);
            self.release_departed_blocks(gpu);
        }

        /// Register the static objects of every interior cell that has just been
        /// baked.
        ///
        /// **It runs here, after `load_block_cells`, and that order is the whole of the wiring.**
        /// The two halves of a cell arrive on two paths in this build — the draw half through
        /// `LandStreamer::bake_env_cells`, the collision half through
        /// `DatLandSource::load_block_cells` — and an object added to a cell the *land source* has
        /// never seen registers no shadow and is silently intangible. The client has no such
        /// split: static-object initialization is a method on the cell.
        ///
        /// A block with no body has no physics world to add to, which is `--no-character`'s flycam and
        /// every headless slice that never logs in; the placements are kept on the block and this
        /// picks them up on the first frame after a body exists.
        ///
        /// **It registers the outdoor population too** — generated landscape scenery and
        /// landblock-info objects. Both populations end in the same body-creation and
        /// cell-insertion pair, with a land cell as the owner. **So
        /// `--no-cell-statics` also makes the landscape's trees intangible.** That is the
        /// switch's own purpose — it is the control that turns this registration off — but the
        /// name is narrower than what it turns off, which matters when reading a differential
        /// taken with it.
        ///
        // **Known limitation:** the cell's static-object initialization entry has no known call
        // site, so *when* the client
        // instantiates a cell's objects is not established. Its refresh branch suggests the
        // visibility path rather than the load path. Doing it per baked block is right if the
        // caller is the load path and merely eager if it is not; nothing observable depends on the
        // difference here, because a `STATIC_PS` object is inert in either way.
        fn init_cell_statics(&mut self, ws: &mut WorldState, store: &RetailDatStore) {
            let registered = world_build::init_cell_statics(
                ws,
                store,
                &self.cfg,
                self.blocks.values_mut().map(|b| &mut b.sim),
            );
            let mut registered: BTreeMap<usize, world_build::StaticBodies> =
                registered.into_iter().collect();
            for (position, block) in self.blocks.values_mut().enumerate() {
                let Some(bodies) = registered.remove(&position) else {
                    continue;
                };
                // **The pairing, and it is one line of indexing because the
                // index was recorded at the bake.** In retail there is nothing to do here:
                // Each static-object slot is one physics body, and its default script is queued
                // *on that same body*. This build
                // builds the collision half in the world builder and the script half in
                // `particles.rs`, so the two are rejoined here, by the slot both were pushed
                // at, never by comparing positions.
                for p in &mut block.emitters {
                    p.body = match p.slot {
                        StaticSlot::Cell(i) => bodies.cell.get(i).copied().flatten(),
                        StaticSlot::Land(i) => bodies.land.get(i).copied().flatten(),
                    };
                }
                // A block whose hosts were spawned before its bodies existed — a scene that
                // ran `sync_objects` with no character attached — gets them now. `placement`
                // is the host's own index into `emitters`, so this is exact and not a match.
                let bodies: Vec<Option<dereth_physics::PhysHandle>> =
                    block.emitters.iter().map(|p| p.body).collect();
                for h in &mut block.hosts {
                    h.body = bodies.get(h.placement).copied().flatten();
                }
            }
        }

        /// **Initialize the house barrier's data half.**
        ///
        /// `dereth_physics::CellRuntime::restriction_obj` is read by the restriction check on every
        /// cell of every transition, and if **nothing writes the field**,
        /// `TransitionCtx::restriction_obj` answers `None` for every cell in the world, the
        /// check returns `OK_TS` before it does anything, and a character walks around inside a
        /// closed property.
        ///
        /// Retail writes it from two places, both of them the CELL dat and neither of them a
        /// message:
        ///
        /// * the landblock static-initialization tail, over the block's land cells, from the
        ///   landblock restriction table;
        /// * per interior cell, from the cell record's own field.
        ///
        /// Both are collected at the bake (see `BlockDraw::cell_restrictions`) and written here.
        fn init_cell_restrictions(&mut self, ws: &mut WorldState) {
            world_build::init_cell_restrictions(ws, self.blocks.values_mut().map(|b| &mut b.sim));
        }

        /// Hand back the descriptor pairs this scene's object textures hold, so that a device can
        /// outlive the scene that used it.
        ///
        /// **The scene must not be drawn again afterwards**; the batches still name slots that are
        /// no longer theirs. It is `WorldScene`'s `Drop`, written as a call because a
        /// `Drop` cannot be handed the `Gpu`.
        ///
        /// This exists because the shader-visible heap is finite -- 32,768 texture pairs -- and
        /// without it nothing releases a dropped scene's textures: many scenes on one device
        /// eventually exhaust the heap. A scene's
        /// interior cells carry their furniture's textures too, which is a real +38% (270 to 372
        /// over the 5x5 blocks around Holtburg). Every entry of
        /// [`BakeCache::surfaces`] is exactly one `Gpu::upload_texture`, whether that upload was a
        /// fresh texture or a cache hit that took a reference, so
        /// releasing one per entry balances the acquires exactly.
        ///
        /// **The merged terrain surfaces go too.** They come from
        /// `LandContext::merge` rather than from [`BakeCache`], and are the rest of a scene's live
        /// slots: over a `long-solo-play`-sized scene, 215 of the 560
        /// pairs a loaded 5x5 window holds. `TerrainMergeCache::drain` supplies the landscape
        /// surface cache's release half; resident blocks' `cell_keys` are dropped at the same time so a
        /// later [`Self::release_departed_blocks`] cannot release a surface this already took.
        ///
        /// Returns how many pairs were handed back.
        pub fn release_textures(&mut self, gpu: &mut Gpu) -> u32 {
            let mut n = 0;
            // **One release per owned SLOT, not per memo entry.** The memo holds
            // exactly one image-texture link per slot however many `GroupKey`s name it and however
            // many consumers it has handed a `TextureSlot` to, so `by_slot`'s keys are the
            // balance. Iterating `surfaces` would release a shared texture once per group.
            // ORDER-OK: every slot is released exactly once and no release can affect another.
            for slot in self.land.bake.by_slot.keys() {
                gpu.release_texture(TextureSlot(*slot));
                n += 1;
            }
            self.land.bake.surfaces.clear();
            // The link book goes with the entries it counts.
            self.land.bake.links.clear();
            self.land.bake.by_slot.clear();
            self.land.bake.key_clip_map.clear();
            self.land.bake.textures_uploaded = 0;
            // **The merged terrain surfaces, which is the other half of the scene.**
            // Every one of them was a `Gpu::upload_texture` (uncached, one link), so one release
            // per handle balances the acquires exactly, the same arithmetic the loop above does
            // for `by_slot`.
            for h in self.land.merge.drain() {
                if h.0 != NO_TEXTURE {
                    gpu.release_texture(TextureSlot(h.0));
                    n += 1;
                }
            }
            // The resident blocks still name those keys. Dropping the names keeps a later
            // `release_departed_blocks` from removing a surface this teardown already took --
            // the same reason `object_meshes` is cleared below.
            for b in self.blocks.values_mut() {
                b.cell_keys.clear();
            }
            for b in &mut self.released_blocks {
                b.cell_keys.clear();
            }
            // The appearances are the largest consumer and their `PartMesh`es name slots that are
            // no longer theirs; dropping them keeps `release_unlinked_appearances` from releasing a
            // second time on a scene that has already been torn down.
            self.object_meshes.clear();
            // **The detail surfaces are the scene's too.** Detail-texturing setup's
            // opening release clears them; a teardown that does not leaves one
            // slot live after the scene is gone.
            n += self.release_detail_textures(gpu);
            n += self.release_terrain_sources(gpu);
            // Another era's sky holds its textures in a cache of its own.
            if let Some(mut cache) = self.sky_cache.take() {
                n += release_bake_cache(&mut cache, gpu);
            }
            // ...and so does another era's look for the objects.
            if let Some(look) = self.land.objects.as_mut() {
                n += release_bake_cache(&mut look.cache, gpu);
            }
            n
        }

        /// Give back what the device-side landscape paths hold: the compositor's resident
        /// sources, the splat draw's cached bindings, and one link per splat source texture.
        /// Returns the number of textures released.
        fn release_terrain_sources(&mut self, gpu: &mut Gpu) -> u32 {
            gpu.reset_merge_sources();
            self.land.gpu_merge_sources.clear();
            // Before the textures go: a cached binding names them.
            gpu.reset_terrain_splat();
            self.land.splats.clear();
            for b in self.blocks.values_mut() {
                b.cell_splat.clear();
            }
            let mut n = 0;
            for slot in std::mem::take(&mut self.land.splat_sources)
                .into_values()
                .flatten()
            {
                gpu.release_texture(slot);
                n += 1;
            }
            n
        }

        /// Every live landscape composite, by merge key, for the tests that compare the device
        /// compositor with the CPU one.
        #[must_use]
        pub fn terrain_composites(&self) -> Vec<(u32, TextureSlot)> {
            self.land
                .merge
                .surfaces()
                .into_iter()
                .filter(|(_, h)| h.0 != NO_TEXTURE)
                .map(|(k, h)| (k.0, TextureSlot(h.0)))
                .collect()
        }

        /// `(cells with splat layers, cells)` over the resident blocks, for the tests.
        #[must_use]
        pub fn terrain_splat_cells(&self) -> (usize, usize) {
            self.blocks.values().fold((0, 0), |(s, n), b| {
                (
                    s + b.cell_splat.iter().filter(|c| c.is_some()).count(),
                    n + b.merge_keys.len(),
                )
            })
        }

        /// Whether the landscape is drawn by splatting. See [`Self::toggle_terrain_splat`].
        #[must_use]
        pub fn terrain_splat(&self) -> bool {
            self.terrain_splat
        }

        /// Whether the ground is palette-shifted, which the ground's region record decides
        /// (`LandSurf` type non-zero): the software region of an older dat set. `false` is texture
        /// merging.
        #[must_use]
        pub fn ground_palette_shifts(&self) -> bool {
            self.land.pal_shift.is_some()
        }

        /// Whether the ground is the world's own (`[Render] Ground` names none, or names the
        /// world's own style).
        #[must_use]
        pub fn ground_is_worlds_own(&self) -> bool {
            self.land.ground.is_none()
        }

        /// Whether the ground's pictures are read from another era's files than the world's.
        #[must_use]
        pub fn ground_from_other_files(&self) -> bool {
            self.land.ground_store.is_some()
        }

        /// The ground style in effect: `None` is the world's own.
        #[must_use]
        pub fn ground_style(&self) -> Option<RegionStyle> {
            self.cfg.render.ground
        }

        /// `[Render] Objects` as drawn: `None` is the world's own.
        #[must_use]
        pub fn objects_style(&self) -> Option<RegionStyle> {
            self.cfg.render.objects
        }

        /// Whether the objects are drawn from another era's files.
        #[must_use]
        pub fn objects_from_other_files(&self) -> bool {
            self.land.objects.is_some()
        }

        /// The other era's files the objects are drawn with and the verdicts of which of their
        /// records stand for the world's; `None` for the world's own look.
        #[must_use]
        pub fn object_look(
            &self,
        ) -> Option<(
            Arc<RetailDatStore>,
            Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
        )> {
            self.land
                .objects
                .as_ref()
                .map(|l| (Arc::clone(&l.files), Arc::clone(&l.identity)))
        }

        /// The terrain types the ground's land surface has a picture for, one bit each.
        #[must_use]
        pub fn ground_drawn_types(&self) -> u32 {
            self.land.drawn
        }

        /// Whether the sky is the world's own.
        #[must_use]
        pub fn sky_is_worlds_own(&self) -> bool {
            self.sky_region.is_none()
        }

        /// Whether the sky's objects are read from another era's files than the world's.
        #[must_use]
        pub fn sky_from_other_files(&self) -> bool {
            self.sky_store.is_some()
        }

        /// The sky style in effect: `None` is the world's own.
        #[must_use]
        pub fn sky_style(&self) -> Option<RegionStyle> {
            self.cfg.render.sky
        }

        /// The region whose sky, light and fog are drawn.
        #[must_use]
        pub fn sky_region(&self) -> &dereth_assets::Region {
            self.sky_region.as_deref().unwrap_or(&self.land.region)
        }

        /// Flip the landscape between its composites and the splat draw, converting the resident
        /// blocks over the next streaming steps. `None` when the device cannot splat, and nothing
        /// changes. The client sets the mode once, from the preferences; this is the tests' way to
        /// exercise both representations on one scene.
        pub fn toggle_terrain_splat(&mut self) -> Option<bool> {
            if !self.land.splat_supported {
                return None;
            }
            self.terrain_splat = !self.terrain_splat;
            self.land.splat_wanted = self.terrain_splat;
            self.land.composites_wanted = !self.terrain_splat;
            Some(self.terrain_splat)
        }

        /// Bring the resident blocks into the current terrain mode after a switch, nearest first
        /// and within [`SceneConfig::stream_budget`]: splatting gives a block its splat layers and
        /// then releases its composites; composites build its composites and then drop its splat
        /// layers. A cell draws whichever it has meanwhile, so a switch never shows a hole. Run
        /// from [`Self::stream`] once a frame; it finds nothing to do once a switch has finished,
        /// because blocks built since carry the current mode's data.
        ///
        /// # Errors
        /// [`WorldError::Render`] when a texture will not upload.
        fn convert_terrain(
            &mut self,
            ws: &WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
        ) -> Result<(), WorldError> {
            let splat = self.terrain_splat;
            let due = |b: &BlockDraw| {
                if splat {
                    b.cell_splat.is_empty() || !b.cell_keys.is_empty()
                } else {
                    (b.cell_keys.is_empty() && !b.merge_keys.is_empty()) || !b.cell_splat.is_empty()
                }
            };
            let mut todo: Vec<(i32, i32)> = self
                .blocks
                .iter()
                .filter(|(_, b)| due(b))
                .map(|(&k, _)| k)
                .collect();
            if todo.is_empty() {
                return Ok(());
            }
            let viewer = ws.streamer.window.viewer_block().unwrap_or((0, 0));
            todo.sort_by_key(|&(bx, by)| {
                let (dx, dy) = (bx - viewer.0, by - viewer.1);
                (dx.abs().max(dy.abs()), dx * dx + dy * dy)
            });
            let started = web_time::Instant::now();
            for (n, k) in todo.into_iter().enumerate() {
                if n > 0
                    && self
                        .cfg
                        .stream_budget
                        .is_some_and(|b| started.elapsed() >= b)
                {
                    break;
                }
                let Some(keys) = self.blocks.get(&k).map(|b| b.merge_keys.clone()) else {
                    continue;
                };
                if splat {
                    let layers = self.land.splat_layers(store, gpu, &keys)?;
                    let refs = self.blocks.get_mut(&k).map(|b| {
                        b.cell_splat = layers;
                        b.cell_texture.clear();
                        std::mem::take(&mut b.cell_keys)
                    });
                    if let Some(refs) = refs {
                        self.release_block_terrain(gpu, &refs);
                    }
                } else {
                    let textures = self.land.composites(store, gpu, &keys)?;
                    if let Some(b) = self.blocks.get_mut(&k) {
                        b.cell_texture = textures;
                        b.cell_keys = keys;
                        b.cell_splat.clear();
                    }
                }
            }
            Ok(())
        }

        /// **Retail's release edge, on this build's appearance cache.**
        ///
        /// Destroying a physics part releases its shift palette and material, restores its
        /// surfaces, and releases both its degrade record and every graphics object in the level
        /// array. The shared cache refuses to decrement a reference count that is not above the
        /// cache's own link, and frees the object once only that link remains. So an object's
        /// geometry and its textures leave when the **last object that links them** leaves, and not
        /// on a clock, a distance or a least-recently-used order.
        ///
        /// [`Self::object_meshes`] is this build's stand-in for the part of that chain that shares
        /// baked geometry between two objects wearing the same recipe, and it holds a **strong**
        /// reference of its own, so nothing would ever reach zero without this. This is that
        /// release: after the
        /// frame's removals, an entry the cache alone holds — `Arc::strong_count == 1`, the exact
        /// analogue of retail's link count being at most 1 — is dropped, and each of its meshes returns the link its
        /// `resolve` took to [`BakeCache::release_group_texture`].
        ///
        /// Returns `(appearances released, descriptor pairs handed back)`.
        fn release_unlinked_appearances(&mut self, gpu: &mut Gpu) -> (usize, u32) {
            // ORDER-OK: the set of keys whose strong count is 1 is order-independent, and every one
            // of them is removed before anything is released, so no order can change the outcome.
            let dead: Vec<AppearanceKey> = self
                .object_meshes
                .iter()
                .filter(|(_, m)| Arc::strong_count(m) == 1)
                .map(|(k, _)| k.clone())
                .collect();
            let mut freed = 0u32;
            for key in &dead {
                let Some(meshes) = self.object_meshes.remove(key) else {
                    continue;
                };
                for part in meshes.iter() {
                    for mesh in part.all() {
                        if let Some(slot) = mesh.texture {
                            self.stats.appearance_texture_releases += 1;
                            if self.release_look_texture(gpu, slot, part.from_look) {
                                freed += 1;
                            } else {
                                self.stats.appearance_textures_still_linked += 1;
                            }
                        }
                    }
                }
            }
            self.stats.appearance_releases += dead.len() as u64;
            // What the sweep chose NOT to free, at the moment it looked. A census with its
            // complement, so "nothing was freed" and "nothing needed freeing" cannot print alike.
            self.stats.appearances_still_linked = self.object_meshes.len();
            (dead.len(), freed)
        }

        /// `object_meshes.len()` **now**, not as of the last `sync_objects`.
        ///
        /// [`SceneStats::server_object_setups`] is written at the end of `sync_objects`, after that
        /// call's creates, so it counts appearances the sweep has not yet been offered. A test that
        /// wants the post-sweep census has to ask for it directly.
        #[must_use]
        pub fn live_appearances(&self) -> usize {
            self.object_meshes.len()
        }

        /// The sampled `BASE1_CLIPMAP` key conflicts, for a test that wants to look at one rather
        /// than only count them. `SceneStats::clipmap_key_conflicts` is the full
        /// count and this is at most [`CLIPMAP_CONFLICT_SAMPLE`] of them.
        #[must_use]
        pub fn clipmap_conflicts(&self) -> &[ClipMapConflict] {
            &self.land.bake.clipmap_conflicts
        }

        /// Appearances the sweep found still linked, and by how many owners each. For a test that
        /// needs to say *why* an entry survived rather than only that it did.
        #[must_use]
        pub fn appearance_link_counts(&self) -> Vec<usize> {
            self.object_meshes.values().map(Arc::strong_count).collect()
        }

        /// How many draw batches one interior cell's baked objects cost, for the tests.
        #[must_use]
        pub fn cell_static_batches(&self, cell: CellId) -> usize {
            self.blocks
                .values()
                .flat_map(|b| &b.env_cells)
                .find(|c| c.id == cell)
                .map_or(0, |c| c.statics.len() + c.statics_blended.len())
        }

        /// One window slot: landblock fetch and generation, and — for a slot that is new rather
        /// than merely resized — dynamic-object, building, and static-object initialization.
        fn build_slot(
            &mut self,
            ws: &mut WorldState,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            xi: u32,
            yi: u32,
            work: SlotWork,
        ) -> Result<(), WorldError> {
            let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                return Ok(());
            };
            let spec = SlotSpec {
                block_x: slot.block_x,
                block_y: slot.block_y,
                lod_div: slot.lod_div,
                dir: slot.trans_dir,
            };
            let (bx, by) = (spec.block_x, spec.block_y);
            let _build =
                tracing::debug_span!("build_landblock", bx, by, lod = spec.lod_div).entered();
            let Some(lb) = read_landblock(store, bx, by) else {
                // Landblock lookup returns no record for a block the dat does not
                // carry -- the world edge. Nothing is drawn there and nothing is an error.
                // The block still has to hand its bake's links back, exactly as one
                // that scrolled off the edge does.
                if let Some(b) = self.blocks.remove(&(bx, by)) {
                    self.released_blocks.push(b);
                    self.release_departed_blocks(gpu);
                }
                return Ok(());
            };
            let (mesh, cell_texture, cell_keys, merge_keys, cell_splat) =
                self.land.generate(store, gpu, &lb, spec)?;
            // Kept before `set_mesh` takes the mesh.
            let side_cell_count = mesh.side_cell_count;

            // Only this bake's requests are wanted below.
            self.land.mip_worker.take_requested();
            let wants_objects = self.wants_objects(ws, xi, yi);
            let previous = self.blocks.remove(&(bx, by));
            // `previous` is either reused whole by the `SlotWork::Mesh` arm below
            // — a re-mesh is a size change, which rebuilds the terrain arrays and keeps the
            // block's objects, so its links stay taken — or thrown away, and a bake that is thrown
            // away has to give its links back before the replacement takes its own. This is the
            // *in-place* half of the same edge `release_departed_blocks` runs for a scroll, and it
            // is the one a `--scenery-radius` change and a re-bake go through.
            // **The third clause is what keeps scenery on a re-entered block.**
            // A bake taken on an outer LOD ring contains **no scenery** —
            // `dereth_world_render::scenery::generate_scenery` opens with the same full-detail
            // guard — and the empty bake must not survive the block's promotion back to
            // full detail. Without this, a block that entered the window coarse keeps its
            // scenery-free bake for the life of the session: `scenery_objects` 333 -> 0 over one
            // out-and-back walk, never recovering, while `static_objects` (which the guard does not
            // gate here) comes back every time and hides it.
            //
            // **The test is `!=`, in BOTH directions, and that is retail and not a choice.**
            // A landblock size-change notification destroys the block's objects on the
            // demotion out of full detail, and rebuilds
            // them only for a block that is at full detail with no static objects (see
            // [`SlotWork`]). A promotion-only rule breaks the invariant the full-detail guard exists to
            // state — *scenery if and only if full
            // detail* — because a block baked at 8 and demoted to 4 then carries 14 to 167 scenery
            // objects on a four-cell mesh. The two halves are one rule.
            //
            // **Declared cost.** At `scenery_radius: 2`, the only shipped configuration in
            // which a resident block can be both baking objects and on a coarse ring, this moves
            // one pixel of 307,200, 53.8 to 54.6 px from the nearest building opening, in pixel
            // differentials whose tolerances were taken over the stale bake.
            let reuse = matches!(work, SlotWork::Mesh)
                && wants_objects
                && previous
                    .as_ref()
                    .is_some_and(|b| b.baked && b.baked_side_cell_count == mesh.side_cell_count);
            let previous = match previous {
                Some(b) if !reuse => {
                    self.released_blocks.push(b);
                    self.release_departed_blocks(gpu);
                    None
                }
                // **The reuse arm returns its TERRAIN links even though it keeps its
                // objects.** `generate` above has already run and taken a fresh reference for
                // every cell of the new mesh; the references the *previous* mesh took are about to
                // be dropped on the floor when `cell_keys` is replaced below. This is the one
                // release site the object half does not have, because an object bake survives
                // a re-mesh (a size change keeps it at an unchanged detail level) and a
                // *terrain* bake never does — `generate` is unconditional.
                //
                // The new references are taken **before** the old ones are returned, deliberately
                // so that a surface the two meshes share is never taken to
                // zero and re-uploaded. Retail's order is the opposite (destroy, then
                // `generate`), which costs it exactly that re-upload; the counts are identical
                // either way and only the peak differs.
                Some(mut b) => {
                    self.stats.blocks_remeshed_in_place += 1;
                    let keys = std::mem::take(&mut b.cell_keys);
                    self.release_block_terrain(gpu, &keys);
                    Some(b)
                }
                None => None,
            };
            let built = match (work, previous) {
                // A resize or a restitch keeps the block's objects — **and this arm is reachable
                // only because `reuse` above has already established that the detail level did not
                // change.** A size change does not rebuild only the terrain arrays; it is at an
                // unchanged `side_cell_count` that
                // retail's objects genuinely are untouched: no size change fires,
                // and the next visibility refresh finds the block's static objects and only
                // re-drops the scene objects' heights and re-crosses their cells.
                (SlotWork::Mesh, Some(b)) if wants_objects && b.baked => BakedObjects {
                    opaque: b.opaque,
                    blended: b.blended,
                    degrade: b.degrade,
                    env_cells: b.env_cells,
                    building_views: b.building_views,
                    scenery: b.scenery,
                    buildings: b.buildings,
                    statics: b.statics,
                    sim: b.sim,
                    cell_lights: b.cell_lights,
                    emitters: b.emitters,
                    hosts: b.hosts,
                    hosts_spawned: b.hosts_spawned,
                    textures_pending: false,
                    objects_visual: b.objects_visual,
                },
                _ if wants_objects => self.land.bake(
                    store,
                    gpu,
                    &lb,
                    &mesh,
                    bx,
                    by,
                    self.cfg.cell_statics,
                    self.cfg.part_degrades,
                    self.cfg.degrade_levels,
                    self.cfg.static_billboards,
                    self.cfg.lod_object_guard,
                )?,
                _ => BakedObjects::default(),
            };

            // A bake that left a texture's mip chain to the worker is handed back whole: its links go
            // back, the block is kept for now as terrain alone -- the state an outer-ring block is
            // in -- and it is queued to bake again once every chain it asked for is done. See
            // [`crate::mip_worker`].
            let (built, baked) = if built.textures_pending {
                let (outside, inside) = Self::baked_texture_slots(&built);
                for (slot, from_look) in outside.into_iter().chain(inside) {
                    self.release_look_texture(gpu, slot, from_look);
                }
                let waiting = self.land.mip_worker.take_requested();
                self.awaiting.insert((bx, by), waiting);
                self.pending.insert((bx, by), SlotWork::Full);
                (BakedObjects::default(), false)
            } else {
                (built, wants_objects)
            };
            ws.streamer.window.set_mesh(xi, yi, SlotMesh::new(mesh));
            self.blocks.insert(
                (bx, by),
                BlockDraw {
                    origin: ws.streamer.window.block_frame_origin(xi, yi),
                    terrain: lb.terrain,
                    cell_texture,
                    // The references `generate` just took, so that whichever of the
                    // two release sites this block eventually leaves through can give them back.
                    cell_keys,
                    merge_keys,
                    cell_splat,
                    opaque: built.opaque,
                    blended: built.blended,
                    degrade: built.degrade,
                    // A block re-meshed by an LOD change keeps its batches and its placements, so
                    // it also keeps its assembly; a freshly baked one has none yet. Either way the
                    // next `refresh_degrade_levels` is what fills it, and it runs before any draw.
                    degrade_assembled: false,
                    env_cells: built.env_cells,
                    building_views: built.building_views,
                    baked,
                    // What the bake above was taken at, so the next `SlotWork::Mesh`
                    // can tell a re-stitch from a change of detail level.
                    baked_side_cell_count: side_cell_count,
                    scenery: built.scenery,
                    buildings: built.buildings,
                    statics: built.statics,
                    sim: built.sim,
                    cell_lights: built.cell_lights,
                    emitters: built.emitters,
                    // Spawned on the next `sync_objects`; a `MotionDriver` needs the shared store.
                    hosts: built.hosts,
                    hosts_spawned: built.hosts_spawned,
                    objects_visual: built.objects_visual,
                },
            );
            Ok(())
        }

        // The residency window and the viewpoint belong to
        // [`dereth_client_runtime::world_stream::WorldStreamer`]: the `LandblockWindow` and the viewer's
        // cell index live on it, and so do the nine reads below. These are the scene's call
        // sites; `recenter`'s one presentation act -- handing the window's `SlotAction`s to the
        // bake queue -- is done here, which is why it is the one that does not simply forward.

        fn wants_objects(&self, ws: &WorldState, xi: u32, yi: u32) -> bool {
            ws.streamer
                .wants_objects(xi, yi, self.cfg.land_radius, self.cfg.scenery_radius)
        }

        /// Queue `work` for the block in window slot `(xi, yi)`. A full build already covers a
        /// re-mesh, so `Full` is never downgraded.
        fn want(&mut self, ws: &WorldState, xi: u32, yi: u32, work: SlotWork) {
            let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                return;
            };
            self.pending
                .entry((slot.block_x, slot.block_y))
                .and_modify(|w| {
                    if work == SlotWork::Full {
                        *w = SlotWork::Full;
                    }
                })
                .or_insert(work);
        }

        /// The window slot currently holding `block`, if it is still in the window.
        fn slot_of_block(&self, ws: &WorldState, (bx, by): (i32, i32)) -> Option<(u32, u32)> {
            let (vx, vy) = ws.streamer.window.viewer_block()?;
            #[allow(clippy::cast_possible_wrap)]
            // LINT-OK: the window radius, at most 25. Not a float conversion.
            let r = ws.streamer.window.mid_radius() as i32;
            let (xi, yi) = (
                u32::try_from(bx - vx + r).ok()?,
                u32::try_from(by - vy + r).ok()?,
            );
            let slot = ws.streamer.window.slot(xi, yi)?;
            ((slot.block_x, slot.block_y) == (bx, by)).then_some((xi, yi))
        }

        /// Whether a block the player can reach -- one inside the scenery radius, where
        /// objects, interiors and their collision live -- is still waiting to be built. The
        /// portal tunnel is held while this is true; blocks further out may keep streaming in
        /// after it ends.
        #[must_use]
        pub fn loading_near_viewer(&self, ws: &WorldState) -> bool {
            self.pending
                .keys()
                .filter_map(|&b| self.slot_of_block(ws, b))
                .any(|(xi, yi)| self.wants_objects(ws, xi, yi))
        }

        fn recenter(&mut self, ws: &mut WorldState) -> RenderSpace {
            let (space, actions) = world_step::recenter(ws, &self.cfg);
            // `Some` only on the path that actually scrolled the window. `queue` derives its
            // departed set from the actions it is handed, so handing it an empty list would
            // release every resident block -- which is why the streamer distinguishes "no
            // actions" from "did not scroll".
            if let Some(actions) = actions {
                self.queue(ws, &actions);
            }
            space
        }

        /// How many blocks currently have geometry, for the tests.
        #[must_use]
        pub fn resident_blocks(&self) -> usize {
            self.blocks.len()
        }

        // -----------------------------------------------------------------------------------
        // **Queue instrumentation.** The six queues this scene defers to a later frame phase.
        // Every one is drained inside `App::frame`, so a settled frame reads zero; a length that
        // never comes back down is a drain that stopped running — which on
        // [`Self::released_blocks`] or [`Self::released_interiors`] is a landblock's bake and
        // interior cells never being handed back at all, i.e. exactly the growth
        // the frame-state latch exists to prevent.
        //
        // Reported one queue at a time rather than as a sum because they fail for six different
        // reasons and a growth station has to name which drain stopped.
        // -----------------------------------------------------------------------------------

        /// How many surface resolves were left pending on the mip worker, ever. See
        /// [`crate::mip_worker`].
        #[must_use]
        pub fn deferred_surfaces(&self) -> u64 {
            self.land.bake.pending_surfaces
        }

        /// Window slots [`Self::stream`] still owes geometry.
        #[must_use]
        pub fn pending_slot_count(&self) -> usize {
            self.pending.len()
        }

        /// Departed bakes waiting to hand their texture links back.
        #[must_use]
        pub fn released_block_count(&self) -> usize {
            self.released_blocks.len()
        }

        /// `(width, height)` of the merged terrain surface the texture merger last composited.
        /// This is
        /// `Render.LandscapeTextureDetail`'s whole observable effect.
        #[must_use]
        pub const fn last_terrain_surface_built(&self) -> Option<(u32, u32, u32)> {
            self.land.merge.last_built
        }

        /// `(width, height, scale)` of the last object texture created — the
        /// observable result of `Render.EnvironmentTextureDetail`.
        #[must_use]
        pub const fn last_object_texture_built(&self) -> Option<(u32, u32, u32)> {
            self.land.bake.last_texture_built
        }

        /// `(uploads, texels uploaded, texels at the source extent, uploads actually downscaled)`
        /// through the object-texture path, cumulative over the session.
        ///
        /// The population `Render.EnvironmentTextureDetail` has to be measured
        /// against, because a single sample can land on the one arm
        /// `dereth_render::texture::scale_surface` declines to resample. The **pair** of texel sums
        /// is what separates "the scale moved" from "the population moved": they are equal by
        /// construction at `FULL_RES`, whatever mix of surfaces the rebuild happened to touch.
        #[must_use]
        pub const fn object_texture_census(&self) -> (u64, u64, u64, u64) {
            (
                self.land.bake.texture_uploads,
                self.land.bake.texture_upload_texels,
                self.land.bake.texture_source_texels,
                self.land.bake.textures_downscaled,
            )
        }

        /// [`crate::render_prefs::RenderPreferences`]'s shadow bank, i.e. what the last preference
        /// poll believes the renderer is running at.
        /// A test that could only read [`Self::cfg`] back would be reading the
        /// value the options page wrote, not the one the renderer acted on.
        #[must_use]
        pub const fn render_shadow(&self) -> crate::render_prefs::RenderPreferences {
            self.render_shadow
        }

        /// **One block's** bake: the two populations counted
        /// separately, the detail level they were taken at, and whether the bake ran at all.
        /// `None` when the block is not resident.
        ///
        /// [`SceneStats::scenery_objects`] is a sum over the window, and a sum cannot say whether
        /// a *particular* block re-baked when it scrolled inward.
        /// [`BlockBake::baked`] is the field that makes a zero a measurement: without it, "this
        /// block's scenery was refused by the full-detail guard" and "this slot is outside
        /// `SceneConfig::scenery_radius` and bakes nothing at all" read identically.
        #[must_use]
        pub fn block_bake(&self, block: (i32, i32)) -> Option<BlockBake> {
            self.blocks.get(&block).map(|b| BlockBake {
                scenery: b.scenery,
                statics: b.statics,
                buildings: b.buildings,
                side_cell_count: b.baked_side_cell_count,
                baked: b.baked,
            })
        }

        /// Every meshed window slot as
        /// `(xi, yi, (block_x, block_y), side_cell_count, the ring's side_cell_count)`.
        ///
        /// The last two are what a seam looks like from a test: a block that scrolled into a
        /// different ring and kept the detail of the one it left.
        #[must_use]
        pub fn window_rings(&self, ws: &WorldState) -> Vec<WindowRing> {
            let w = ws.streamer.window.mid_width();
            let expected = ws.streamer.window.ring_map();
            let mut out = Vec::new();
            for xi in 0..w {
                for yi in 0..w {
                    let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                        continue;
                    };
                    let Some(mesh) = slot.mesh.as_ref().and_then(SlotMesh::get::<LandblockMesh>)
                    else {
                        continue;
                    };
                    #[allow(clippy::cast_possible_wrap)]
                    // LINT-OK: window indices, bounded by mid_width <= 31. Not a float conversion.
                    let (a, b) = (xi as i32, yi as i32);
                    out.push(WindowRing {
                        xi: a,
                        yi: b,
                        block: (slot.block_x, slot.block_y),
                        side_cell_count: mesh.side_cell_count,
                        expected_side_cell_count: expected[(w * xi + yi) as usize],
                    });
                }
            }
            out
        }

        /// How many of the resident blocks' baked batches are on the **alpha list** — the ones
        /// the alpha-list renderer would queue rather than draw in place.
        ///
        /// Exists so a test of [`SceneConfig::portal_alpha_flush`] can say whether the flush had
        /// anything to move: a differential over an empty queue proves nothing, and this is what
        /// distinguishes "the flush changed no pixel" from "there was nothing to flush".
        #[must_use]
        pub fn alpha_list_batches(&self) -> usize {
            self.blocks
                .values()
                .flat_map(|b| &b.blended)
                .filter(|b| static_alpha_list_member(b))
                .count()
        }

        /// Every resident degrading placement as level selection last saw it: the
        /// distance handed to
        /// `get_degrade` (the viewer distance over the z scale), the level it answered, and the
        /// record it was asked. **A test's comparison, and the only reader outside this file.**
        ///
        /// It publishes the *input* as well as the answer deliberately: a census that reported
        /// only the levels could agree with itself and disagree with the record, and the drawn
        /// level has to be checked against `get_degrade`'s own answer.
        #[must_use]
        pub fn part_degrade_probe(&self, ws: &WorldState) -> Vec<PartLevelProbe> {
            let globals = self.degrade_globals(ws);
            let cam = detail_viewer(ws);
            let share = self.degrade.governor.level();
            let mut out = Vec::new();
            let mut push = |object: Option<ObjectId>,
                            origin: Vec3,
                            outdoors: bool,
                            shared: Option<(f32, Vec3)>,
                            is_player: bool,
                            parts: &[dereth_animation::parts::PhysicsPart],
                            meshes: &[PartLevels],
                            levels: &[u32],
                            draw: &[Frame]| {
                for (i, pl) in meshes.iter().enumerate() {
                    let Some(part) = parts.get(i) else { continue };
                    let chosen = part_level(pl, part, is_player, cam, shared, &globals);
                    let (distance, want, mode) = (chosen.distance, chosen.level, chosen.mode);
                    // The heading is the third input `calc_draw_frame` was given, from the same
                    // measurement as the distance: the object's when it shares, the part's own
                    // otherwise.
                    let heading = chosen.heading;
                    out.push(PartLevelProbe {
                        mode,
                        cypt: chosen.cypt,
                        sort_center: pl.sort_center,
                        gfxobj_scale: part.gfxobj_scale,
                        gfxobj: pl.gfxobj,
                        pos: part.pos,
                        draw_pos: draw.get(i).copied().unwrap_or(part.pos),
                        viewer_heading: heading,
                        object,
                        object_origin: origin,
                        shared: shared.is_some(),
                        outdoors,
                        part: i,
                        is_player,
                        distance,
                        // What the scene is **drawing**, which is what `refresh_part_levels`
                        // stored last frame -- not `want`, which is what it would choose now.
                        // A probe that recomputed the answer it is checking would agree with
                        // itself for ever; `want` is published beside it so a test can see both.
                        level: levels.get(i).copied().unwrap_or(0),
                        would_choose: want,
                        record: pl.info.as_ref().map(|r| r.id),
                        levels_held: pl.levels.len(),
                    });
                }
            };
            for (id, o) in &ws.objects {
                let d = self.object_draw(*id);
                push(
                    Some(*id),
                    o.frame.origin,
                    ws.object_draw_cell(o)
                        .is_some_and(dereth_physics::landdefs::is_outdoors),
                    object_shared_distance(ws, o, cam, &share),
                    o.is_player,
                    &o.sim.driver.borrow().part_array.parts,
                    &d.meshes,
                    &d.part_levels,
                    &d.part_draw_pos,
                );
            }
            if let Some(c) = ws.character.as_ref() {
                let driver = c.driver();
                push(
                    None,
                    self.render_frame_of(ws, c.position()).origin,
                    dereth_physics::landdefs::is_outdoors(c.position().cell),
                    self.character_shared_distance(ws, cam, &share),
                    true,
                    &driver.part_array.parts,
                    &self.character_parts,
                    &self.character_part_levels,
                    &self.character_part_draw_pos,
                );
            }
            out
        }

        /// The equivalent for the baked statics, with the
        /// billboarding half: what each resident placement was measured at, what level it drew,
        /// and what draw-frame calculation did to its frame.
        ///
        /// It publishes the lookup's *input* because a census reporting only the
        /// levels could agree with itself and disagree with the record, and it publishes
        /// `pos` beside `draw_pos` for the same reason one level out: a probe that reported only
        /// the mode would be asserting that `select_levels` decoded the record, not that anything
        /// on the device turned.
        #[must_use]
        pub fn degrade_probe(&self, ws: &WorldState) -> Vec<StaticLevelProbe> {
            let cam = detail_viewer(ws);
            let share_2dsq = self.degrade.governor.level().share_distance_2dsq(false);
            let mut out = Vec::new();
            for block in self.blocks.values() {
                let viewer = Vec3::new(cam.x - block.origin.0, cam.y - block.origin.1, cam.z);
                let outside = block.degrade.iter().map(|p| (p, true));
                let cells = block
                    .env_cells
                    .iter()
                    .flat_map(|c| c.degrade.iter().map(|p| (p, false)));
                for (p, outdoors) in outside.chain(cells) {
                    let (cypt, heading) =
                        placement_viewer_distance(p, outdoors, viewer, share_2dsq);
                    let shared = p.object_origin.is_some_and(|o| {
                        dereth_world_render::objects::parts::object_viewer_distance(
                            o, outdoors, viewer, share_2dsq,
                        )
                        .is_some()
                    });
                    let z = p.scale_z;
                    out.push(StaticLevelProbe {
                        cypt,
                        object_origin: p.object_origin,
                        viewer,
                        outdoors,
                        distance: if z != 0.0 { cypt / z } else { cypt },
                        level: p.level,
                        record: p.info.id,
                        mode: p.mode,
                        billboards: p.billboards,
                        pos: p.frame,
                        draw_pos: p.draw_pos,
                        viewer_heading: heading,
                        shared,
                    });
                }
            }
            out
        }

        /// The force-level console override that `get_degrade` reads in its second branch, pinning
        /// **every** object to one level.
        ///
        /// An independent reference audit found a single read of this value and no writer. Its
        /// sibling `degrades_disabled` has three writers, and the degrade multiplier has its own
        /// setter, so the absence is specific to this override. It initializes to -1 and never
        /// moves in the client, so the behavior "pin every object to level n" is dormant there;
        /// this describes the observed behavior rather than a missing wire in this rebuild.
        ///
        /// It is settable here so the branch is reachable from the scene's own per-frame path and
        /// can be asserted end to end through [`Self::degrade_probe`], rather than only in
        /// `get_degrade`'s unit tests. **It is not a producer** and no production code calls it;
        /// the thing that pins level 0 in a running client is `degrades_disabled`, which the map
        /// and overhead cameras raise (`Self::degrades_disabled`).
        pub fn set_force_level(&mut self, level: i32) {
            self.force_level = level;
        }

        /// The current, -1 when nothing is pinned. See
        /// [`Self::set_force_level`].
        #[must_use]
        pub fn force_level(&self) -> i32 {
            self.force_level
        }

        /// The three degrade counters and the drawn triangle count, recomputed after
        /// every level selection so that the numbers a test reads are this frame's.
        fn refresh_degrade_stats(&mut self) {
            let placements = || {
                self.blocks.values().flat_map(|b| {
                    b.degrade
                        .iter()
                        .chain(b.env_cells.iter().flat_map(|c| c.degrade.iter()))
                })
            };
            self.stats.degrade_placements = placements().count();
            self.stats.degrade_placements_degraded = placements().filter(|p| p.level != 0).count();
            self.stats.degrade_placements_culled = placements()
                .filter(|p| {
                    p.info
                        .degrades
                        .get(p.level as usize)
                        .is_none_or(|e| e.gfxobj_id.0 == 0)
                })
                .count();
            // "The record billboards somewhere" and "the level being drawn asks for
            // a billboard" are different facts, and the second is the one this row reports. It is
            // deliberately **not** gated on `p.billboards`, so the count is the same in both arms
            // of `SceneConfig::static_billboards` and the switch cannot flatter itself.
            self.stats.degrade_placements_billboarding =
                placements().filter(|p| p.billboards).count();
            self.stats.degrade_placements_billboarded = placements()
                .filter(|p| p.mode != dereth_world_render::objects::degrade::DegradeMode::None)
                .count();
            self.stats.object_triangles = self
                .blocks
                .values()
                .flat_map(|b| b.opaque.iter().chain(&b.blended))
                .map(|b| drawn_vertices(b).len() / (3 * LAND_VERTEX_STRIDE as usize))
                .sum();
        }

        /// Recount what the resident blocks add up to. The counters are a sum over the window, so
        /// they change with every scroll rather than being fixed at load.
        fn refresh_land_stats(&mut self, ws: &mut WorldState) {
            self.refresh_surface_stats();
            self.stats.blocks_meshed = self.blocks.len();
            self.stats.terrain_surfaces = self.land.merge.len();
            // Read from the cache rather than accumulated here, so the number a test
            // asserts on is the merge cache's own count and not a second copy of it.
            self.stats.terrain_unowned_releases = self.land.merge.unowned_removes;
            self.stats.terrain_surfaces_built = self.land.merge.surfaces_built;
            self.stats.terrain_surface_requests = self.land.merge.surface_requests;
            self.stats.scenery_objects = self.blocks.values().map(|b| b.scenery).sum();
            self.stats.buildings = self.blocks.values().map(|b| b.buildings).sum();
            self.stats.static_objects = self.blocks.values().map(|b| b.statics).sum();
            self.stats.object_batches = self
                .blocks
                .values()
                .map(|b| b.opaque.len() + b.blended.len())
                .sum();
            self.stats.object_batches_untextured = self
                .blocks
                .values()
                .flat_map(|b| b.opaque.iter().chain(&b.blended))
                .filter(|b| b.texture.is_none())
                .count();
            // What one frame *draws*, which for a chunked batch is the assembled
            // subset. `object_triangles_resident` below is what the bake *holds*.
            self.stats.object_triangles = self
                .blocks
                .values()
                .flat_map(|b| b.opaque.iter().chain(&b.blended))
                .map(|b| drawn_vertices(b).len() / (3 * LAND_VERTEX_STRIDE as usize))
                .sum();
            self.stats.object_triangles_resident = self
                .blocks
                .values()
                .flat_map(|b| b.opaque.iter().chain(&b.blended))
                .map(|b| b.vertices.len() / (3 * LAND_VERTEX_STRIDE as usize))
                .sum();
            self.refresh_degrade_stats();
            self.stats.cell_static_batches = self
                .blocks
                .values()
                .flat_map(|b| &b.env_cells)
                .map(|c| c.statics.len() + c.statics_blended.len())
                .sum();
            // The interior half of `object_triangles_resident`, held rather than
            // drawn, so that the cost a released block hands back is readable in the same terms
            // as the other degrade counters.
            self.stats.cell_static_triangles_resident = self
                .blocks
                .values()
                .flat_map(|b| &b.env_cells)
                .flat_map(|c| c.statics.iter().chain(&c.statics_blended))
                .map(|b| b.vertices.len() / (3 * LAND_VERTEX_STRIDE as usize))
                .sum();
            self.stats.cell_statics = ws.cell_static_objects.stats;
            self.stats.upload_bytes = self.worst_case_upload_bytes();
        }

        /// Put a body in the scene and point the camera at it.
        ///
        /// Everything about the body is [`Character`](dereth_client_runtime::character::Character)'s; what happens here is the *drawing* half —
        /// each part's graphics object triangulated once ([`dereth_client_runtime::models`]) and its surface resolved to
        /// a pipeline state ([`resolve_surface`]). Both happen before the first frame, for the same
        /// reason the terrain does: `Gpu::upload_texture` is a one-shot command list of its own and
        /// creating one mid-frame is a needless flush.
        ///
        /// # Errors
        /// [`WorldError`] when the character cannot be created or a device resource fails.
        pub fn attach_character(
            &mut self,
            ws: &mut WorldState,
            store: &std::sync::Arc<RetailDatStore>,
            region: &dereth_assets::Region,
            gpu: &mut Gpu,
        ) -> Result<(), WorldError> {
            // The body, its preferences, its sound table, its place in the world and its start
            // cell are the world builder's; what happens here between them is the *drawing* half.
            let character = world_build::body_for(store, region, &self.cfg)
                .map_err(|e| WorldError::Render(e.to_string()))?;
            // Is the table a `SoundTable` animation hook
            // resolves its `SoundType` against — reads the
            // **object's own**, which for the body is the one installed.
            ws.character_sound_table = world_build::body_sound_table(&character);
            let array = character.driver().part_array.parts.clone();
            // This array *is* `player_iid`'s, so the degrade guard is not applied.
            let parts =
                self.build_object_meshes(store, gpu, Some(character.setup_id()), &array, true)?;
            self.set_character_parts(gpu, parts);
            // The window was built before the body existed, so nothing has prefetched its blocks'
            // interior cells into *this* land source yet.
            let resident: Vec<(i32, i32)> = self.blocks.keys().copied().collect();
            world_build::place_body(ws, character, resident);
            // For the same reason: the blocks already in the window were baked with no
            // physics world to add their cells' objects to.
            self.init_cell_statics(ws, store);
            // Deliberately *outside* `init_cell_statics` and outside its
            // `SceneConfig::cell_statics` switch: a house barrier is not a static object, and
            // `--no-cell-statics` is the control for the solidity of the landscape, not for
            // who may walk onto a property.
            self.init_cell_restrictions(ws);
            world_build::stand_at_start_cell(ws, store, &self.cfg);
            self.stats.upload_bytes = self.worst_case_upload_bytes();
            self.refresh_surface_stats();
            ws.follow_character();
            Ok(())
        }

        /// Record what the body costs to draw. Split out because the meshes are rebuilt
        /// whenever the server's `ObjDesc` for the player changes.
        /// Install a freshly baked body and **release the set it replaces**.
        ///
        /// Replacing a part releases its old graphics-object array before it loads the new one,
        /// so in the
        /// original the old meshes' links go back on the very edge that installs the new ones,
        /// through the same resource-release path
        /// [`Self::release_unlinked_appearances`] describes. Dropping the old `Vec<PartLevels>`
        /// on the floor instead would strand, on every appearance change on the
        /// player, the slots its previous outfit held.
        ///
        /// The new set is built by the caller **before** this runs, so a texture both outfits
        /// share is never taken to zero and re-uploaded: the incoming links are already counted
        /// when the outgoing ones are returned.
        fn set_character_parts(&mut self, gpu: &mut Gpu, parts: Vec<PartLevels>) {
            for part in std::mem::take(&mut self.character_parts) {
                for mesh in part.all() {
                    if let Some(slot) = mesh.texture {
                        self.stats.body_texture_releases += 1;
                        if self.release_look_texture(gpu, slot, part.from_look) {
                            self.stats.body_textures_freed += 1;
                        }
                    }
                }
            }
            // `character_parts`, `character_batches` and `character_triangles` are
            // what the body **draws**, so they are written by [`Self::refresh_part_levels`] once
            // a level has been chosen, not here where none has been. What is written here is the
            // resident cost, which is a property of the bake.
            //
            // The three drawn counters are seeded with level 0's answer so that a scene built and
            // read without a frame reports the bake rather than zero; the first
            // `refresh_part_levels` replaces them.
            self.stats.character_triangles_resident =
                parts.iter().map(PartLevels::triangles_held).sum();
            self.stats.character_parts = parts.iter().filter(|p| !p.at(0).is_empty()).count();
            self.stats.character_batches = parts.iter().map(|p| p.at(0).len()).sum();
            self.stats.character_triangles = parts.iter().map(|p| p.triangles_at(0)).sum();
            self.character_part_levels = vec![0; parts.len()];
            self.character_part_cypt = vec![0.0; parts.len()];
            self.character_parts = parts;
        }

        /// Apply the server's `ObjDesc` to the player's own body.
        ///
        /// The player is the one object with **no** [`SceneObject`]: the scene builds him locally
        /// from `ALUVIAN_MALE_SETUP` before the server says anything, and skips drawing
        /// the server's copy so the parts are not doubled. So his appearance has to be applied to
        /// the local part array instead, which is what applying description changes does to the
        /// *one* physics body the client keeps for the player.
        ///
        /// An `AnimPartChange` names a **part index**, so an index into a setup the body was not
        /// built from dresses the wrong limb. **That is fixed upstream instead of refused**: the
        /// body is rebuilt from the setup record the server's `0xF745` names
        /// ([`Character::set_setup_id`](dereth_client_runtime::character::Character::set_setup_id)) and the description then applies to *its* indices.
        /// [`SceneStats::objdesc_setup_mismatch`] is the guard — it counts only a
        /// description refused because the server's own setup record would not build — and it is
        /// expected to be zero across the whole capture corpus.
        ///
        /// Refusing instead would drop the entire description of any player who is not an Aluvian
        /// male, which is three of the five in-world captures: their body would keep
        /// `0x02000001`'s parts and the loincloth setup creation gave it.
        /// Materialize pending removals/creates and the local player's setup/appearance before
        /// dispatching movement. Shared by full scene synchronization and App's ordered player
        /// motion/teleport stage: a create must install its movement manager before a later F74C,
        /// and a target created earlier in the batch must exist when MoveToObject resolves it.
        ///
        /// The simulation is the world's ([`world_objects::prepare_object_dispatch`]); this
        /// scene's drawing half follows it through [`DrawFollows`] at the points it has to:
        /// removals, the appearance cache's sweep, each created object's part meshes, and the
        /// body's meshes when its parts change.
        ///
        /// Called from the application **before** `begin_frame`: creating a texture
        /// resets a command list of its own, and although `dereth_render` retires the old upload
        /// arena behind the fence rather than freeing it under an open list, keeping every device
        /// resource creation outside the frame bracket is still the rule this crate follows.
        ///
        /// # Errors
        /// [`WorldError`] when a device resource cannot be created.
        pub fn prepare_object_dispatch(
            &mut self,
            ws: &mut WorldState,
            store: &Arc<RetailDatStore>,
            gpu: &mut Gpu,
            stream: &mut ObjectStream,
        ) -> Result<(), WorldError> {
            let cfg = self.cfg;
            let mut counters = world_objects::ObjectCounters::default();
            let done = world_objects::prepare_object_dispatch(
                ws,
                store,
                &cfg,
                stream,
                &mut counters,
                &mut DrawFollows {
                    draw: self,
                    gpu,
                    store,
                },
            );
            self.fold_object_counters(counters);
            done
        }

        /// Give pending objects geometry, then apply positions and movement.
        ///
        /// Called outside the device frame bracket. App may already have run the shared create
        /// prefix for ordered player dispatch; drained creates/removals do not run twice. Other
        /// scene callers retain the complete synchronization path.
        ///
        /// # Errors
        /// [`WorldError`] when a device resource cannot be created.
        pub fn sync_objects(
            &mut self,
            ws: &mut WorldState,
            store: &Arc<RetailDatStore>,
            gpu: &mut Gpu,
            stream: &mut ObjectStream,
        ) -> Result<(), WorldError> {
            let cfg = self.cfg;
            let mut counters = world_objects::ObjectCounters::default();
            let done = world_objects::sync_objects(
                ws,
                store,
                &cfg,
                stream,
                &mut counters,
                &mut DrawFollows {
                    draw: self,
                    gpu,
                    store,
                },
            );
            self.fold_object_counters(counters);
            done?;
            self.stats.server_object_setups = self.object_meshes.len();
            // `server_object_batches` and `server_object_triangles` are what one
            // frame **draws**, so they are written by [`Self::refresh_part_levels`]. Seeded here
            // with level 0's answer, and with the resident cost, so that a scene synchronised but
            // never stepped reports the bake rather than zero.
            self.stats.server_object_batches = self
                .object_draws
                .values()
                .map(|o| o.meshes.iter().map(|p| p.at(0).len()).sum::<usize>())
                .sum();
            self.stats.server_object_triangles = self
                .object_draws
                .values()
                .map(|o| o.meshes.iter().map(|p| p.triangles_at(0)).sum::<usize>())
                .sum();
            self.stats.server_object_triangles_resident = self
                .object_draws
                .values()
                .map(|o| {
                    o.meshes
                        .iter()
                        .map(PartLevels::triangles_held)
                        .sum::<usize>()
                })
                .sum();
            // The scripted statics become live objects and every emitter gets its
            // mesh, here rather than in `stream` because a `MotionDriver` needs the shared store.
            self.bring_up_particles(ws, store, gpu)?;
            self.stats.upload_bytes = self.worst_case_upload_bytes();
            self.refresh_surface_stats();
            Ok(())
        }

        /// Object dispatch's counters, among the scene's own.
        fn fold_object_counters(&mut self, c: world_objects::ObjectCounters) {
            self.fold_step_stats(c.steps);
            self.stats.remote_sticks_applied += c.steps.remote_sticks_applied;
            self.stats.remote_sticks_unresolved += c.steps.remote_sticks_unresolved;
            self.stats.remote_move_tos_performed += c.steps.remote_move_tos_performed;
            self.stats.objdesc_setup_mismatch += c.objdesc_setup_mismatch;
            self.stats.objdesc_failures += c.objdesc_failures;
            self.stats.objdescs_applied += c.objdescs_applied;
            self.stats.object_nodraw_set += c.object_nodraw_set;
            self.stats.object_nodraw_cleared += c.object_nodraw_cleared;
            self.stats.object_hidden_changed += c.object_hidden_changed;
            self.stats.scripts_played += c.scripts_played;
            self.stats.scripts_unplayed += c.scripts_unplayed;
            self.stats.vector_updates_applied += c.vector_updates_applied;
            self.stats.vector_updates_deferred += c.vector_updates_deferred;
            self.stats.vector_updates_without_body += c.vector_updates_without_body;
            self.stats.character_states_applied += c.character_states_applied;
            self.stats.restriction_effects_disabled += c.restriction_effects_disabled;
            self.stats.restriction_effects_unplayed += c.restriction_effects_unplayed;
            self.stats.restriction_effects_played += c.restriction_effects_played;
            if let Some(n) = c.server_objects {
                self.stats.server_objects = n;
            }
            if let Some(n) = c.server_objects_animated {
                self.stats.server_objects_animated = n;
            }
        }

        /// Give every scripted placement a live object, and every emitter a mesh.
        ///
        /// Called from [`Self::sync_objects`] because that is the frame step handed both the
        /// shared `Arc<RetailDatStore>` a `MotionDriver` needs and the device the meshes need, and
        /// because `Gpu::upload_texture` runs a command list of its own and so may not be called
        /// inside the frame bracket.
        ///
        /// # Errors
        /// [`WorldError::Render`] when a device resource cannot be created.
        fn bring_up_particles(
            &mut self,
            ws: &mut WorldState,
            store: &Arc<RetailDatStore>,
            gpu: &mut Gpu,
        ) -> Result<(), WorldError> {
            let assets = match &ws.anim_assets {
                Some(a) => Arc::clone(a),
                None => {
                    let a = Arc::new(DatAnimAssets::new(Arc::clone(store)));
                    ws.anim_assets = Some(Arc::clone(&a));
                    a
                }
            };
            let dyn_assets: Arc<dyn dereth_animation::data::AnimAssets> = Arc::clone(&assets) as _;

            // --- the hosts ----------------------------------------------------------------
            // Host creation is deferred one frame from the bake.
            for (&(bx, by), block) in &mut self.blocks {
                if block.hosts_spawned {
                    continue;
                }
                block.hosts_spawned = true;
                #[allow(clippy::cast_precision_loss)] // a block index, 0..=254
                let (ox, oy) = (bx as f32 * BLOCK_LENGTH, by as f32 * BLOCK_LENGTH);
                for (slot, p) in block.emitters.iter().enumerate() {
                    let Some(setup) =
                        dereth_animation::data::AnimAssets::setup(assets.as_ref(), p.setup)
                    else {
                        continue;
                    };
                    // The setup's default sound-table id, which
                    // `MotionDriver::set_setup` does not read. Taken before `set_setup` moves the
                    // record.
                    let sound_table = setup.default_sound_table;
                    let mut driver = MotionDriver::new(Arc::clone(&dyn_assets));
                    // Static-object creation receives `(setup_id, 0, 0)`: object id 0 and **not**
                    // dynamic.
                    // Setup creation queues the default script on the object's script manager;
                    // its hooks run on the first script update.
                    if !driver.set_setup(setup) {
                        continue;
                    }
                    // The placement in **absolute** world coordinates. See
                    // [`crate::particles::collect`]: a particle born with `is_parent_local == 0`
                    // keeps its birth frame for its whole life, and a permanent emitter's whole
                    // life outlasts every landblock scroll.
                    let frame = Frame::new(
                        Vec3::new(
                            p.frame.origin.x + ox,
                            p.frame.origin.y + oy,
                            p.frame.origin.z,
                        ),
                        p.frame.rotation,
                    );
                    // Physics placement would put it in a cell; the script layer only asks
                    // whether it is in one, and a placed static always is.
                    driver.env.in_cell = true;
                    driver.env.position = Position::new(CellId(0), frame);
                    // Place the part array through the internal part update:
                    // a placed object's parts are in world space from the moment it is placed.
                    driver.update_parts(&frame);
                    // `p.body` is this placement's `static_objects[i]`, already
                    // filled if [`Self::init_cell_statics`] has run for the block (`stream` runs
                    // before `sync_objects` in `App::frame`, so it normally has); `placement` is
                    // how `init_cell_statics` reaches back to a host that was spawned first,
                    // which is the order a scene with no character yet takes.
                    block.hosts.push(EmitterHost {
                        cell: p.cell,
                        driver,
                        frame,
                        placement: slot,
                        body: p.body,
                        sound_table,
                    });
                }
            }

            // --- the meshes ---------------------------------------------------------------
            // One per `hw_gfxobj_id`: emitter setup builds `max_particles` parts from that
            // one id, so every particle of an emitter draws the same graphics object.
            let mut wanted: Vec<DataId> = Vec::new();
            {
                let want = |m: &dereth_animation::ParticleManager, out: &mut Vec<DataId>| {
                    for e in m.iter() {
                        let id = e.info.hw_gfxobj_id;
                        if !self.particle_gfx.contains(id) && !out.contains(&id) {
                            out.push(id);
                        }
                    }
                };
                for block in self.blocks.values() {
                    for h in &block.hosts {
                        want(&h.driver.particles, &mut wanted);
                    }
                }
                for o in ws.objects.values() {
                    want(&o.sim.driver.borrow().particles, &mut wanted);
                }
                if let Some(c) = ws.character.as_ref() {
                    want(&c.driver().particles, &mut wanted);
                }
            }
            for id in wanted {
                let groups = dereth_client_runtime::models::build_gfxobj(store, id);
                let entry = if groups.is_empty() {
                    None
                } else {
                    let textures = TextureStore::with_environment_texture_detail(
                        store,
                        self.cfg.render.environment_texture_detail,
                    );
                    let meshes = build_meshes(
                        store,
                        &mut self.land.bake,
                        &textures,
                        gpu,
                        &groups,
                        None,
                        None,
                    )
                    .map_err(|e| WorldError::Render(e.to_string()))?;
                    Some(ParticleGfx {
                        meshes,
                        sort_center: crate::particles::read_sort_center(store, id),
                        degrade: crate::particles::read_degrade(store, id),
                        // The same a part carries:
                        // view-cone checking places it and light minimization chooses this
                        // particle's lights against it.
                        drawing_sphere: self.land.bake.drawing_sphere(store, id),
                    })
                };
                self.particle_gfx.insert(id, entry);
            }
            self.stats.emitter_hosts = self.blocks.values().map(|b| b.hosts.len()).sum();
            Ok(())
        }

        /// Route one landblock static's queued animation-hook events to the
        /// systems that can apply them.
        ///
        /// # What the original client does with a static's hooks
        ///
        /// Static-object initialization creates a physics body, and insertion registers that same
        /// body in its cell and landblock. Setup creation queues the default script on the body's
        /// script manager, the physics tick animates it, and hook execution applies effects
        /// directly to it: play a sound, set ethereal state, change scale, and so on. The original
        /// client has no hand-back queue between those operations.
        ///
        /// # What this build's hosts can honour
        ///
        /// Each [`crate::particles::EmitterHost`] is a `MotionDriver`, cell and world frame, paired
        /// with the existing placement body and carrying the setup's default sound-table id.
        /// Consequently direct waves, table sounds and body-scale hooks reach their consumers;
        /// driver-owned particle operations remain applied in the driver.
        ///
        /// The remaining body operations and child-script arm are counted rather than
        /// applied, although the handle is available, because their receivers have not been
        /// wired through this seam. The light and texture-velocity variants have no receiver, and
        /// shipped-data negative controls still make new nonzero counts visible.
        ///
        /// See [`HostHookStats`] for the per-arm reasons and shipped counts.
        ///
        /// # Why a routine and not a wildcard
        ///
        /// An explicit empty arm is evidence; a wildcard is not.
        /// Every variant is named here,
        /// including variants that are negative controls,
        /// including the ones the retail data
        /// never raises on a static, so that the next reader can see the set was read rather than
        /// skipped — and so that a new variant is a compile error rather than a silent drop.
        fn drain_host_hooks(
            driver: &mut dereth_animation::MotionDriver,
            at: Vec3,
            body: Option<dereth_physics::PhysHandle>,
            world: Option<&mut dereth_physics::PhysicsWorld>,
            table: Option<DataId>,
            pending_sound: &mut Vec<SoundTrigger>,
            stats: &mut HostHookStats,
        ) {
            let mut world = world;
            for e in driver.take_events() {
                stats.drained += 1;
                match e {
                    // Sound and tweaked-sound hooks play sound with
                    // `(gid, obj[, prio, prob, vol])`.
                    // The object is only the emitter's *position*, which a host has, so this one
                    // crosses whole. `at` is the renderer's viewer-block-relative space, which is
                    // what `WorldScene::listener` measures against — a host's own frame is
                    // absolute, so the caller adds the block shift.
                    dereth_animation::AnimEvent::PlaySound {
                        gid,
                        volume,
                        priority,
                        probability,
                    } => {
                        stats.sounds += 1;
                        pending_sound.push(SoundTrigger::Wave {
                            id: gid,
                            at,
                            volume,
                            priority,
                            probability,
                        });
                    }
                    // Sound-table hook handling resolves the `sound_type` against the
                    // **object's own** sound table, which for a placed static is the one
                    // setup creation installed from the setup's default sound-table field.
                    // The host carries that id,
                    // so this takes the same `SoundTrigger::Table` route `execute_anim_hooks`
                    // gives a server object. A host whose setup names no table is retail's own
                    // dead end, counted rather than dropped.
                    dereth_animation::AnimEvent::PlaySoundType { kind } => {
                        if let Some(table) = table {
                            stats.sound_types += 1;
                            pending_sound.push(SoundTrigger::Table {
                                table,
                                stype: kind.0,
                                at,
                            });
                        } else {
                            stats.sound_types_no_table += 1;
                        }
                    }
                    // Reports. `dereth_animation` created, stopped or destroyed the emitter inside
                    // `execute_hook`, and resolved both arms of `CallPES` itself.
                    dereth_animation::AnimEvent::CreateParticleEmitter { .. }
                    | dereth_animation::AnimEvent::DestroyParticleEmitter(_)
                    | dereth_animation::AnimEvent::StopParticleEmitter(_) => {
                        stats.applied_in_driver += 1;
                    }
                    dereth_animation::AnimEvent::CallPes { pause, .. } => {
                        stats.applied_in_driver += 1;
                        stats.pes_hooks += 1;
                        stats.pes_hooks_delayed +=
                            u64::from(pause >= dereth_primitives::num::consts::EPSILON);
                    }
                    // **The scale store.** `SetScale`'s immediate arm
                    // updates both the part array, which `dereth_animation` already did inside the driver,
                    // and `scale` on the object. It is the same assignment `execute_anim_hooks`
                    // makes for a server object — one line, on the handle the pairing found.
                    //
                    // Retail performs no cross-cell recalculation afterwards, so a grown
                    // static keeps the cell shadows it registered at its old size, in retail too
                    // (a retail limit, deliberately not "fixed").
                    dereth_animation::AnimEvent::SetScale(s) => {
                        stats.scale_hooks += 1;
                        match body.and_then(|h| world.as_deref_mut().and_then(|w| w.get_mut(h))) {
                            Some(o) => o.scale = s,
                            None => stats.scale_hooks_no_body += 1,
                        }
                    }
                    // These body operations and the child-list script operation are not routed
                    // through this seam. All five are
                    // raised by nothing in the shipped data, so this arm
                    // is the read set and not a live route; a non-zero counter is new data.
                    dereth_animation::AnimEvent::SetEthereal(_)
                    | dereth_animation::AnimEvent::SetNoDraw(_)
                    | dereth_animation::AnimEvent::SetOmega(_)
                    | dereth_animation::AnimEvent::Attack { .. }
                    | dereth_animation::AnimEvent::PlayDefaultScript { .. } => {
                        stats.no_body += 1;
                    }
                    // No receiver exists anywhere in the client; both are explicit negative controls.
                    dereth_animation::AnimEvent::SetLights(_)
                    | dereth_animation::AnimEvent::SetTextureVelocity { .. } => {
                        stats.unreceivable += 1;
                    }
                    dereth_animation::AnimEvent::ReplaceObjectNoOp { .. } => stats.retail_noop += 1,
                    dereth_animation::AnimEvent::MotionDone { .. } => stats.motion_done += 1,
                }
            }
        }

        /// How many animation events are sitting undrained on the landblock statics' queues.
        ///
        /// **A census sampler**, in the shape the long-session growth checks
        /// use: a container length a long-running station can read after every cycle and compare
        /// with its own baseline. It must be flat. Undrained, it would be the session length
        /// times the rate at which the shipped ambient scripts fire, and a self-looping
        /// `CALL_PES` makes that rate non-zero for ever.
        #[must_use]
        pub fn host_pending_events(&self) -> usize {
            self.blocks
                .values()
                .flat_map(|b| b.hosts.iter())
                .map(|h| h.driver.pending_events())
                .sum()
        }

        /// **The pairing, for a test to read.** Every landblock static that runs a script,
        /// with the collision body that is the *same placement* — in retail one static-object
        /// slot, which is one physics body carrying both halves.
        ///
        /// The two origins are given in the same absolute world space the host's frame is in, so
        /// that "this handle is this host's body" is a float comparison the caller can make
        /// exactly: both come from one `EmitterPlacement::frame`, so any mismatch is a pairing bug
        /// and not a rounding one.
        #[must_use]
        pub fn host_bodies(&self, ws: &WorldState) -> Vec<HostBody> {
            let mut out = Vec::new();
            for (&(bx, by), block) in &self.blocks {
                #[allow(clippy::cast_precision_loss)] // a block index, 0..=254
                let (ox, oy) = (bx as f32 * BLOCK_LENGTH, by as f32 * BLOCK_LENGTH);
                for h in &block.hosts {
                    let o = h.body.and_then(|b| {
                        ws.character
                            .as_ref()
                            .and_then(|c| c.world.get(b))
                            .map(|o| (b, o))
                    });
                    out.push(HostBody {
                        cell: h.cell,
                        setup: block
                            .emitters
                            .get(h.placement)
                            .map_or(DataId(0), |p| p.setup),
                        at: h.frame.origin,
                        body: h.body,
                        body_at: o.map(|(_, o)| {
                            let p = o.position.frame.origin;
                            Vec3::new(p.x + ox, p.y + oy, p.z)
                        }),
                        body_cell: o.map(|(_, o)| o.position.cell),
                        scale: o.map(|(_, o)| o.scale),
                        radius: o.map(|(_, o)| o.radius()),
                        sound_table: h.sound_table,
                    });
                }
            }
            out
        }

        /// The particle half of the physics tick, run for every object that has a particle manager.
        ///
        /// The hosts' script manager is ticked here too, because a placed static has no animation
        /// and therefore never went through `step_animation`; the server's objects and the body
        /// have already had theirs ticked by `Self::advance_objects` and
        /// [`dereth_client_runtime::character::Character::update`].
        fn update_particles(&mut self, ws: &mut WorldState, now: dereth_primitives::LocalTime) {
            let viewer = detail_viewer(ws);
            let shift = self.block_shift(ws);
            // The particle draw decision is per **emitter**, because its argument is that
            // emitter's own `degrade_distance`. The animation crate's `MotionDriver::update_particles`
            // takes one flag for the whole manager, so the widest of an object's emitters decides
            // here. See [`manager_degrade_distance`] for what that costs and who owns it.
            let cut = manager_degrade_distance;
            // The cell test. This renderer draws every resident block's objects without a per-cell
            // frustum test, so an emitter that is drawn at all is treated as being in a visible
            // cell; the distance half of the should-draw-particles test is exact.
            let in_view = Some(dereth_world_render::cells::cull::Bounding::PartiallyInside);

            let mut live = 0usize;
            let mut emitters = 0usize;
            // `self` is split so that the host loop can hold the body's
            // `PhysicsWorld` mutably at the same time as the blocks: retail's scale hook writes
            // `scale` on the very object whose script manager raised the hook, and here the two
            // halves of that object live in different fields.
            let WorldState {
                character,
                pending_sound,
                ..
            } = ws;
            let Self {
                blocks,
                particle_gfx,
                stats,
                ..
            } = self;
            for block in blocks.values_mut() {
                for h in &mut block.hosts {
                    h.driver.cur_time = ServerTime(now.0);
                    // Place the parts **before** running scripts.
                    // A particle-creation hook names a part index and
                    // captures that part's frame as the birth frame, which for an emitter with
                    // `is_parent_local == 0` is where its particles live for ever. The client's
                    // order is the same — part placement completes before the first script update.
                    h.driver.update_parts(&h.frame);
                    h.driver.update_scripts();
                    let origin = h.frame.origin.add(shift);
                    let cypt = origin.sub(viewer).mag2().sqrt();
                    let draw = dereth_world_render::particles::should_draw_particles(
                        false,
                        cypt,
                        cut(particle_gfx, &h.driver.particles),
                        in_view,
                    );
                    h.driver.update_particles(draw);
                    // Update interpolated hook values, which
                    // static-object animation runs directly after the script and
                    // particle updates. This is the interpolating half of the six ramp hooks.
                    h.driver.update_fp_hooks();
                    // And the queue those hooks just filled. Retail has no
                    // queue: hook handling calls straight into the physics body and the
                    // step is over. Here the hooks become events, and for the statics this is what
                    // takes them off again.
                    //
                    // It is handed the rest of the object: the body the placement
                    // pairing found and the sound table the setup named.
                    Self::drain_host_hooks(
                        &mut h.driver,
                        origin,
                        h.body,
                        character.as_mut().map(|c| &mut c.world),
                        h.sound_table,
                        pending_sound,
                        &mut stats.hosts,
                    );
                    emitters += h.driver.particles.len();
                    live += h
                        .driver
                        .particles
                        .iter()
                        .map(|e| e.live().count())
                        .sum::<usize>();
                }
            }
            for o in ws.objects.values_mut() {
                let cypt = o.frame.origin.sub(viewer).mag2().sqrt();
                let draw = dereth_world_render::particles::should_draw_particles(
                    false,
                    cypt,
                    cut(&self.particle_gfx, &o.sim.driver.borrow().particles),
                    in_view,
                );
                o.sim.driver.borrow_mut().update_particles(draw);
                // As above — `process_hooks` also closes
                // the internal position update, which is a server object's own tick.
                o.sim.driver.borrow_mut().update_fp_hooks();
                emitters += o.sim.driver.borrow().particles.len();
                live += o
                    .sim
                    .driver
                    .borrow()
                    .particles
                    .iter()
                    .map(|e| e.live().count())
                    .sum::<usize>();
            }
            if let Some(c) = ws.character.as_ref() {
                let render = c.render_frame();
                let mut d = c.driver_mut();
                d.cur_time = ServerTime(now.0);
                // **Particle updating, for the body.**
                //
                // Particle updating runs for **every** physics body, and this
                // build holds three kinds: a block's scripted statics get
                // it four lines above, and `ws.objects` get it through `advance_objects` ->
                // [`step_animation`] and `finish_object_physics`. The **player's own body** gets it
                // here. [`dereth_client_runtime::character::Character::update`] is this build's per-frame update for
                // the body and it runs `tick_movement` and `process_hooks` and stops.
                //
                // Without this, `play_script_type`'s player branch would queue the script — and
                // answer `true` — and its
                // particle-creation hooks would never execute. The level-up script is played on the
                // player's own object in **every** recording that has one, so this one call is what
                // makes level-up particles appear.
                //
                // Here rather than in `Character::update` because `cur_time` is set on the line
                // above and the script manager schedules against it: pumping the scripts from a place
                // that has not stamped the clock runs every hook at the previous frame's time.
                // That is the same reason the `block.hosts` arm above pumps them here too.
                d.update_scripts();
                let cypt = render.origin.sub(viewer).mag2().sqrt();
                let draw = dereth_world_render::particles::should_draw_particles(
                    false,
                    cypt,
                    cut(&self.particle_gfx, &d.particles),
                    in_view,
                );
                d.update_particles(draw);
                // **The body's own `process_hooks`.**
                //
                // The hidden script's transparency hook is `1.0 -> 1.0 over 0.0 s`, which turns
                // straight into `NoDraw`; the unhide script's is `1.0 -> 0.0 over 0.75 s`, a timed
                // interpolation hook. Without this call
                // the second one is discarded, so the `set_hidden` script arm can take
                // the body away and nothing gives it back: the body stays invisible after
                // initial login.
                d.update_fp_hooks();
                emitters += d.particles.len();
                live += d.particles.iter().map(|e| e.live().count()).sum::<usize>();
            }
            self.stats.particles.emitters = emitters;
            self.stats.particles.live = live;
            self.stats.particles.meshes = self.particle_gfx.len();
        }

        /// The region record the scene was built from, held in the shared cache for the session.
        /// The ambient-sound scan reads it.
        #[must_use]
        pub fn region(&self) -> &dereth_assets::Region {
            &self.land.region
        }

        /// The `GfxObj` ids the body's meshes on the device were **baked from**, in part order.
        /// See [`Self::character_built_from`].
        ///
        /// A test comparing this against the capture's own `part_index -> part_id` map is
        /// asserting that the geometry uploaded is the geometry the `ObjDesc` asked for, rather
        /// than that `part_array.parts[i].gfxobj_id` was updated. The two disagree exactly when a
        /// description (or a setup rebuild) lands after the bake, the same failure the paper
        /// doll can have.
        #[must_use]
        pub fn character_built_from(&self) -> &[DataId] {
            &self.character_built_from
        }

        /// The descriptor-heap slot each of the body's baked batches binds, per part.
        ///
        /// The texture half of an `ObjDesc` never changes a part's `GfxObj`, so
        /// [`Self::character_built_from`] cannot see it: the surface substitution
        /// reaches the device only as a *different texture* on the same geometry. These are the
        /// slots `Gpu::bind_texture` is handed, allocated out of one cache per scene — so two
        /// bakes of the same body in the same scene are directly comparable, and a part whose
        /// texture the description changed shows a different slot. Anything else is a claim about
        /// `PhysicsPart::surface_overrides`, which is the model.
        #[must_use]
        pub fn character_part_textures(&self) -> Vec<Vec<u32>> {
            self.character_parts
                .iter()
                .enumerate()
                .map(|(i, pl)| {
                    // The level actually drawn, so this stays a claim about the
                    // geometry on the device rather than about level 0's.
                    pl.at(self.character_part_levels.get(i).copied().unwrap_or(0))
                        .iter()
                        .filter_map(|m| m.texture.map(|t| t.0))
                        .collect()
                })
                .collect()
        }

        /// The viewer's landblock neighbourhood, which supplies ambient-sound input.
        ///
        /// The **3×3 landblock neighbourhood** around the viewer — the blocks whose
        /// orientation LOD is 1 — 8×8 terrain cells each, each cell contributing
        /// its south-west corner vertex as the sound position and its terrain word as the type and
        /// scene. A block meshed at a coarser LOD ring has no per-cell terrain to contribute and
        /// the client would not have walked it either, so it is skipped rather than interpolated.
        #[must_use]
        pub fn terrain_neighbourhood(&self, ws: &WorldState) -> dereth_audio::TerrainNeighbourhood {
            use dereth_audio::{TerrainCell, TerrainNeighbourhood};
            let viewer = ws
                .streamer
                .window
                .viewer_block()
                .unwrap_or_else(|| block_xy(self.cfg.landblock));
            let mut cells = Vec::with_capacity(9 * 64);
            for dy in -1..=1i32 {
                for dx in -1..=1i32 {
                    let key = (viewer.0 + dx, viewer.1 + dy);
                    let Some(block) = self.blocks.get(&key) else {
                        continue;
                    };
                    let Some(mesh) = self.mesh_of(ws, key) else {
                        continue;
                    };
                    if mesh.side_cell_count != 8 {
                        continue;
                    }
                    for row in 0..8usize {
                        for col in 0..8usize {
                            let v = mesh.vertices[mesh.vertex_index(row, col)];
                            cells.push(TerrainCell {
                                pos: Vec3::new(v.x + block.origin.0, v.y + block.origin.1, v.z),
                                terrain_word: block.terrain[row * 9 + col],
                            });
                        }
                    }
                }
            }
            // The position-change gate: the scan runs only when the viewer's cell is
            // outdoors. Inside a dungeon the landscape contributes nothing and every ambient fades
            // — dungeons are ambient-silent.
            let outdoors = ws.viewer_cell().is_none();
            TerrainNeighbourhood { outdoors, cells }
        }

        /// The mesh a resident block was built with, by block coordinate.
        fn mesh_of<'w>(&self, ws: &'w WorldState, block: (i32, i32)) -> Option<&'w LandblockMesh> {
            let mid = ws.streamer.window.mid_width();
            for xi in 0..mid {
                for yi in 0..mid {
                    let slot = ws.streamer.window.slot(xi, yi)?;
                    if (slot.block_x, slot.block_y) == block {
                        return slot.mesh.as_ref().and_then(SlotMesh::get::<LandblockMesh>);
                    }
                }
            }
            None
        }

        /// The animation and physics-script sound hooks raised since the last call.
        ///
        /// Sound, sound-table, and tweaked-sound hook execution belongs to the animation crate; the hooks
        /// reach this crate as
        /// [`dereth_animation::AnimEvent`]s, and the position each one plays at is the emitting object's
        /// own `position`, which is what retail plays the sound at.
        ///
        /// **This does not drain the queues.** [`Self::process_hooks`] is the single place the
        /// drivers' queues are emptied, because another consumer must not depend on
        /// this one being called: `world_use_time` returns early when there is no audio device,
        /// and a door must not stay solid on a machine with no sound card. This call keeps its
        /// shape and its contract —
        /// the triggers of the frame just stepped, in order — and everything the physics seam does
        /// not want is still dropped.
        pub fn take_sound_events(&mut self, ws: &mut WorldState) -> Vec<SoundTrigger> {
            self.process_hooks(ws);
            std::mem::take(&mut ws.pending_sound)
        }

        /// The special-heritage appearance Apply prelude:
        /// destroy the local player's particle manager, then play the selected crown/setup PES.
        /// This deliberately changes no `ObjDesc`; the server's later appearance update owns it.
        pub fn replace_player_particle_script(
            &mut self,
            ws: &mut WorldState,
            script: DataId,
        ) -> bool {
            let Some(character) = ws.character.as_ref() else {
                return false;
            };
            let played = {
                let mut driver = character.driver_mut();
                driver.particles = dereth_animation::ParticleManager::default();
                // Heritage 10's two no-crown PES globals are native INVALID_DID/zero. Apply still
                // destroys the old manager, then skips PlayScript; zero is not a failed PES.
                if script.0 == 0 {
                    return true;
                }
                driver.play_script_id(script)
            };
            if played {
                self.stats.scripts_played += 1;
            } else {
                self.stats.scripts_unplayed += 1;
            }
            played
        }

        /// Drain the drivers' queues and run this step's animation hooks. The drain lives in
        /// [`dereth_client_runtime::anim_hooks`]; this is the
        /// call site: the drain needs the scene's sound sink and its drawn origins, which it
        /// takes through [`dereth_client_runtime::anim_hooks::HookObject`].
        pub fn process_hooks(&mut self, ws: &mut WorldState) {
            world_step::process_hooks(ws, &mut self.stats);
        }

        /// Absolute world coordinates → the renderer's viewer-block-relative space.
        ///
        /// Landscape rendering draws with the south-west corner of the **viewer's**
        /// landblock at the origin, and that block moves as the player walks.
        fn block_shift(&self, ws: &WorldState) -> Vec3 {
            dereth_client_runtime::landblock::block_shift(
                ws.streamer.window.viewer_block(),
                self.cfg.landblock,
            )
        }

        /// Live particles owned by the requested pass-local cells.
        fn collect_particles(
            &self,
            ws: &WorldState,
            visible_cells: Option<&BTreeSet<u32>>,
            include_outdoors: bool,
        ) -> Vec<ParticlePart> {
            let shift = self.block_shift(ws);
            let mut out = Vec::new();
            // Parent assignment makes the emitter inherit
            // the host cell. `PARTICLE_EMITTER_PS` adds the particle shadow to the cell:
            // exactly ONE cell, not the ordinary object's overlap set. Its live parts can
            // reach cell drawing only if that cell was reached. Keeping every resident emitter
            // in this global collection drew the villa's unseen basement flames on its floor.
            let drawn_cells = self.frame_drawn_cells.borrow();
            let cells = visible_cells.or(drawn_cells.as_ref());
            let visible = |cell: CellId| {
                (include_outdoors && dereth_physics::landdefs::is_outdoors(cell))
                    || cells.is_some_and(|cells| cells.contains(&cell.0))
            };
            for block in self.blocks.values() {
                for h in &block.hosts {
                    if visible(h.cell) {
                        // The emitter's own cell decides which pass lights its
                        // particles, the same split `PartSubmission::outdoors` carries.
                        let outdoors = dereth_physics::landdefs::is_outdoors(h.cell);
                        crate::particles::collect(&h.driver.particles, shift, outdoors, &mut out);
                    }
                }
            }
            // The server's objects and the body are simulated in the renderer's own space, because
            // that is the space their parts are placed in and their emitters follow the placed
            // frames, so they need no shift. A window scroll moves that space, and the re-centre
            // moves their live particles with it (`world_step::rebase_render_space_particles`).
            for o in ws.objects.values() {
                if let Some(cell) = ws.object_draw_cell(o).filter(|c| visible(*c)) {
                    let outdoors = dereth_physics::landdefs::is_outdoors(cell);
                    crate::particles::collect(
                        &o.sim.driver.borrow().particles,
                        Vec3::ZERO,
                        outdoors,
                        &mut out,
                    );
                }
            }
            if let Some(c) = ws.character.as_ref() {
                let cell = c.position().cell;
                if visible(cell) {
                    let outdoors = dereth_physics::landdefs::is_outdoors(cell);
                    crate::particles::collect(
                        &c.driver().particles,
                        Vec3::ZERO,
                        outdoors,
                        &mut out,
                    );
                }
            }
            out
        }

        /// Copy the shared surface cache's counters into [`SceneStats`].
        ///
        /// They are counters on *tolerant* paths — a sub-palette range that would not apply, a
        /// palette that is not in the dat — and this project has twice paid for a counter nothing
        /// compared, so `tests/gpu/objects/appearance_objdesc.rs` asserts both are zero over the whole corpus.
        fn refresh_surface_stats(&mut self) {
            self.stats.palette_range_failures = self.land.bake.palette_range_failures;
            self.stats.palette_missing = self.land.bake.palette_missing;
            self.stats.textures_uploaded = self.land.bake.textures_uploaded;
            self.stats.parts_not_drawn = self.land.bake.parts_not_drawn;
            self.stats.surfaces_resolved = self.land.bake.surfaces_resolved;
            self.stats.surfaces_translucent = self.land.bake.surfaces_translucent;
            self.stats.texture_key_hits = self.land.bake.texture_key_hits;
            self.stats.solid_texel_key_hits = self.land.bake.solid_texel_key_hits;
            self.stats.solid_texels_uploaded = self.land.bake.solid_texels_uploaded;
            self.stats.solid_colour_words = self.land.bake.solid_colour_words.len();
            self.stats.clipmap_key_conflicts = self.land.bake.clipmap_key_conflicts;
            self.stats.bake_unowned_releases = self.land.bake.unowned_releases;
        }

        /// [`Self::build_part_meshes`] for a server object or the body, with the objects' look.
        ///
        /// Each part is built wholly from one era
        /// ([`dereth_client_runtime::models::parts_for_look`]): from the look's files through the
        /// look's surface cache, with its texture changes carried onto the look's pictures, when
        /// the look may stand for it; from `world` otherwise, and every part when the look is the
        /// world's own. Each part records which cache holds its pictures. `setup` is the setup the
        /// array was built on, which tells a remodel the look draws with its own parts.
        fn build_object_meshes(
            &mut self,
            world: &RetailDatStore,
            gpu: &mut Gpu,
            setup: Option<DataId>,
            array: &[dereth_animation::parts::PhysicsPart],
            is_player: bool,
        ) -> Result<Vec<PartLevels>, WorldError> {
            let Some((files, identity)) = self
                .land
                .objects
                .as_ref()
                .map(|l| (Arc::clone(&l.files), Arc::clone(&l.identity)))
            else {
                return self.build_part_meshes(world, gpu, array, is_player);
            };
            let chosen = dereth_client_runtime::models::parts_for_look(
                world, &files, &identity, setup, array,
            );
            let from_look = chosen.iter().filter(|p| p.is_some()).count();
            self.stats.object_parts_from_look += from_look as u64;
            self.stats.object_parts_from_world += (chosen.len() - from_look) as u64;
            if from_look == 0 {
                self.stats.object_appearances_from_world += 1;
                return self.build_part_meshes(world, gpu, array, is_player);
            }
            self.stats.object_appearances_from_look += 1;
            // One part at a time, each through its own era's files and cache; the body's record of
            // what each part was built from is gathered across them.
            let mut parts = Vec::with_capacity(array.len());
            let mut built_from = Vec::new();
            for (part, look) in array.iter().zip(&chosen) {
                let mut built = match look {
                    // A part the world draws beside the look's parts takes the look's colours
                    // where the look has them, so the object is one colour across its parts.
                    None => match dereth_client_runtime::models::colours_for_look(
                        &files, &identity, part,
                    ) {
                        Some((coloured, ranges)) => self.build_part_meshes_coloured(
                            world,
                            gpu,
                            std::slice::from_ref(&coloured),
                            is_player,
                            Some((&files, ranges)),
                        )?,
                        None => self.build_part_meshes(
                            world,
                            gpu,
                            std::slice::from_ref(part),
                            is_player,
                        )?,
                    },
                    Some(p) => {
                        // The build reads the scene's surface cache; for the look it is the
                        // look's, put in its place for the length of the build and given back
                        // whatever the build returns.
                        let lk = self.land.objects.as_mut().expect("the look is drawn");
                        let mut cache = std::mem::take(&mut lk.cache);
                        std::mem::swap(&mut self.land.bake, &mut cache);
                        let built =
                            self.build_part_meshes(&files, gpu, std::slice::from_ref(p), is_player);
                        std::mem::swap(&mut self.land.bake, &mut cache);
                        self.land.objects.as_mut().expect("the look is drawn").cache = cache;
                        let mut built = built?;
                        for b in &mut built {
                            b.from_look = true;
                        }
                        built
                    }
                };
                if is_player {
                    built_from.append(&mut self.character_built_from);
                }
                parts.append(&mut built);
            }
            if is_player {
                self.character_built_from = built_from;
            }
            Ok(parts)
        }

        /// Turn a setup's part array into drawable meshes, exactly as [`Self::attach_character`]
        /// does for the body. Object space; the transform rides in the `PerDraw` block.
        ///
        /// This runs every part through the near-band guard, so the degrade guard applies to
        /// a creature's part array exactly as it does to a baked static — but
        /// the viewer-distance update consults the degrade record only for a degrading part of an
        /// object other than the player, and uses level 0 otherwise, so the
        /// **local player alone** never consults the record and draws `gfxobj[0]`, which for a
        /// marker is the marker itself. The exemption is the client's, not a convenience here.
        fn build_part_meshes(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            array: &[dereth_animation::parts::PhysicsPart],
            is_player: bool,
        ) -> Result<Vec<PartLevels>, WorldError> {
            self.build_part_meshes_coloured(store, gpu, array, is_player, None)
        }

        /// [`Self::build_part_meshes`], with `colours`, the other era's files and which colour
        /// ranges of the parts' descriptions are read there
        /// ([`TextureStore::with_colours_from`]).
        fn build_part_meshes_coloured(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            array: &[dereth_animation::parts::PhysicsPart],
            is_player: bool,
            colours: Option<(&RetailDatStore, Vec<bool>)>,
        ) -> Result<Vec<PartLevels>, WorldError> {
            let mut textures = TextureStore::with_environment_texture_detail(
                store,
                self.cfg.render.environment_texture_detail,
            );
            if let Some((look, ranges)) = colours {
                textures = textures.with_colours_from(look, ranges);
            }
            let mut parts: Vec<PartLevels> = Vec::new();
            // Recorded here, in the loop that actually reads `part.gfxobj_id` and hands
            // it to `build_gfxobj`, so that it is the *drawn* geometry's id and not the model's.
            // Only for the local body, which is the one array whose meshes are re-baked in place;
            // a `SceneObject` keeps its own `Arc<Vec<Vec<PartMesh>>>` per appearance.
            if is_player {
                self.character_built_from.clear();
            }
            for part in array {
                // The part draw guard `g = gfxobj[deg_level]; if (!g) return`, at
                // the near band, for everything that is not the local player. The part keeps its
                // slot and contributes **no** mesh: the index is the part index every frame's part
                // update writes into, so dropping the entry would shift the
                // whole body.
                let drawn = is_player
                    || !self.cfg.part_degrades
                    || self.land.bake.draws_at_near_band(store, part.gfxobj_id);
                if !drawn {
                    self.land.bake.parts_not_drawn += 1;
                }
                // `part.gfxobj_id` is already the swapped one —
                // ran before this — and `part.surface_overrides` is the texture-map and shift
                // palette half, which reaches the decode through [`build_meshes`].
                //
                // A refused part takes the **empty** group list rather than an early `continue`,
                // and the loop therefore pushes exactly one entry per part whatever it decides:
                // `parts[i]` draws `part_array.parts[i]`, and skipping the push would silently
                // slide every later part's mesh onto the wrong bone. That is written as a shape
                // rather than as an assertion because no capture-driven test in this tree catches
                // it -- it was mutated and stayed green.
                if !drawn {
                    if is_player {
                        self.character_built_from.push(part.gfxobj_id);
                    }
                    // Even a part the near-band guard refused carries its sort centre:
                    // the viewer-distance update measures the distance from
                    // the graphics object's sort centre whatever drawing later decides, and a probe
                    // that reported `(0,0,0)` here would be reporting the bake's shortcut rather
                    // than the client's value.
                    let sort_center = self.land.bake.sort_center(store, part.gfxobj_id);
                    // And its sphere, for the same reason. A part the near-band guard
                    // refused has no meshes, so `should_draw_part` never offers it and the sphere
                    // is never asked for — but reading it here keeps `spheres` and `levels` the
                    // same length on every branch, which is what makes `drawing_sphere_at`'s
                    // index the same index as `at`'s.
                    let sphere = self.land.bake.drawing_sphere(store, part.gfxobj_id);
                    parts.push(PartLevels::single(
                        part.gfxobj_id,
                        sort_center,
                        sphere,
                        Vec::new(),
                    ));
                    continue;
                }
                // Fills `gfxobj[i]` from
                // `degrades[i].gfxobj_id` and the viewer-distance update re-picks `i`
                // every frame, so a part with a record contributes **one mesh set per level**
                // and [`Self::refresh_part_levels`] chooses between them.
                //
                // The **local player is built with every level too**, and deliberately: the
                // exemption is the viewer-distance update's check that the object is not the player,
                // which is a decision about *selection*, not about what the graphics-object array loads.
                // Building his array one level long would put the exemption in the geometry,
                // where no test could tell it from a body that simply has no other level; built
                // this way, "the player does not degrade" is a level index a test reads.
                let record = if self.cfg.part_degrades && self.cfg.part_degrade_levels {
                    self.land.bake.degrade_record(store, part.gfxobj_id)
                } else {
                    None
                };
                let build = |scene: &mut Self, gfxobj: DataId, gpu: &mut Gpu| {
                    let groups = build_gfxobj(store, gfxobj);
                    build_meshes(
                        store,
                        &mut scene.land.bake,
                        &textures,
                        gpu,
                        &groups,
                        part.surface_overrides.as_ref(),
                        None,
                    )
                    .map_err(|e| WorldError::Render(e.to_string()))
                };
                let Some(info) = record else {
                    if is_player {
                        self.character_built_from.push(part.gfxobj_id);
                    }
                    // **The sort centre is read on this branch too.** A part with no
                    // `GfxObjDegradeInfo` (the large majority) given
                    // `sort_center = (0,0,0)` would have its viewer distance measured to the part's
                    // **origin** rather than to the graphics object's sort centre, which the depth
                    // sort then gets wrong. Viewer-distance updating reads
                    // the graphics object's sort centre unconditionally.
                    let sort_center = self.land.bake.sort_center(store, part.gfxobj_id);
                    // Level 0's drawing sphere, the sphere mesh drawing runs
                    // `viewcone_check` on. Read through the same memo as the sort centre, and out of
                    // the same graphics object.
                    let sphere = self.land.bake.drawing_sphere(store, part.gfxobj_id);
                    parts.push(PartLevels::single(
                        part.gfxobj_id,
                        sort_center,
                        sphere,
                        build(self, part.gfxobj_id, gpu)?,
                    ));
                    continue;
                };
                // As on the static side: **229 of the
                // 4,131 shipped records name a level-0 mesh that is not the object pointing at
                // them**, so the id the part *carries* and the id level 0 *draws* are not always
                // the same. `character_built_from` is documented as what the mesh was baked from,
                // so it records level 0's.
                if is_player {
                    self.character_built_from.push(info.degrades[0].gfxobj_id);
                }
                let mut levels = Vec::with_capacity(info.degrades.len());
                let mut spheres = Vec::with_capacity(info.degrades.len());
                for e in &info.degrades {
                    // `gfxobj_id == 0` is `gfxobj[i] == NULL`: selecting that level draws
                    // nothing, which is different from falling back to the previous one.
                    if e.gfxobj_id.0 == 0 {
                        levels.push(Vec::new());
                        spheres.push(None);
                    } else {
                        levels.push(build(self, e.gfxobj_id, gpu)?);
                        // **per level**, because mesh drawing is handed
                        // the selected level's graphics object and reads the sphere off it. A degrade level is a
                        // different mesh with a different extent, so level 0's sphere would be the
                        // wrong cone test for every other level.
                        spheres.push(self.land.bake.drawing_sphere(store, e.gfxobj_id));
                    }
                }
                // The sort centre belongs to level 0's graphics object and is read before any level is picked.
                let sort_center = self
                    .land
                    .bake
                    .sort_center(store, info.degrades[0].gfxobj_id);
                parts.push(PartLevels {
                    levels,
                    gfxobj: info.degrades[0].gfxobj_id,
                    sort_center,
                    info: Some(info),
                    spheres,
                    from_look: false,
                });
            }
            Ok(parts)
        }

        /// A server position, expressed in the renderer's viewer-block-relative space.
        ///
        /// The same transform [`Character::render_frame`](dereth_client_runtime::character::Character::render_frame) applies: a `Position`'s origin is
        /// landblock-local, and the scene's origin is the south-west corner of the block it was
        /// built around (the current landscape viewpoint block).
        fn render_frame_of(&self, ws: &WorldState, pos: Position) -> Frame {
            world_step::render_frame_of(ws, self.cfg.landblock, pos)
        }

        /// [`object_shared_distance`] for the local body, measured from its render-space origin.
        fn character_shared_distance(
            &self,
            ws: &WorldState,
            cam: Vec3,
            share: &dereth_world_render::degrade_loop::DegradeLevel,
        ) -> Option<(f32, Vec3)> {
            let c = ws.character.as_ref()?;
            let position = c.position();
            let origin = self.render_frame_of(ws, position).origin;
            dereth_world_render::objects::parts::object_viewer_distance(
                origin,
                dereth_physics::landdefs::is_outdoors(position.cell),
                cam,
                share.share_distance_2dsq(false),
            )
        }

        /// Project a target's selection sphere for `VividTargetIndicator`.
        /// This uses the setup selection sphere, not a part's drawing sphere or the
        /// selected-part visibility latch. The default-view setup deliberately ignores portals.
        #[must_use]
        pub fn target_projection(
            &self,
            ws: &WorldState,
            id: ObjectId,
            viewport: (u32, u32),
        ) -> Option<dereth_client_contract::target::Projection> {
            use dereth_client_contract::target::Projection;
            use dereth_physics::math::{globaltolocal, localtoglobal};
            use dereth_world_render::cells::{
                clip::ViewPoly,
                cull::{viewcone_check, Bounding},
            };
            let object = ws.objects.get(&id)?;
            let driver = object.sim.driver.borrow();
            let parts = &driver.part_array;
            // Scale the centre componentwise,
            // but the radius by Z (not max-axis). The no-setup fallback
            // has centre {0,0,0.1} AND radius 0.1, as retail's own values establish.
            let (center, radius) =
                parts
                    .setup
                    .as_ref()
                    .map_or((Vec3::new(0.0, 0.0, 0.1), 0.1), |s| {
                        let c = s.selection_sphere.center;
                        (
                            Vec3::new(
                                c.x * parts.scale.x,
                                c.y * parts.scale.y,
                                c.z * parts.scale.z,
                            ),
                            s.selection_sphere.radius * parts.scale.z,
                        )
                    });
            let world_center = localtoglobal(&object.frame, center);
            let view = self.view_params(ws, viewport.0, viewport.1);
            let eye = self.eye_transform(ws, &view);
            let cone = ViewPoly::full_screen(eye.width, eye.height, &eye);
            let camera = ws.camera.frame();
            if viewcone_check(
                world_center,
                radius,
                &self.viewer_near_plane(ws),
                &cone.planes,
            ) != Bounding::Outside
            {
                // The camera's right/up expressed in object
                // coordinates, then centre - right*r + up*r and centre + right*r - up*r.
                let rotation = Frame::new(Vec3::ZERO, object.frame.rotation);
                let camera_rotation = Frame::new(Vec3::ZERO, camera.rotation);
                let right = globaltolocal(
                    &rotation,
                    localtoglobal(&camera_rotation, Vec3::new(1.0, 0.0, 0.0)),
                );
                let up = globaltolocal(
                    &rotation,
                    localtoglobal(&camera_rotation, Vec3::new(0.0, 0.0, 1.0)),
                );
                let a = Vec3::new(
                    (center.x - right.x * radius) + up.x * radius,
                    (center.y - right.y * radius) + up.y * radius,
                    (center.z - right.z * radius) + up.z * radius,
                );
                let b = Vec3::new(
                    (center.x + right.x * radius) - up.x * radius,
                    (center.y + right.y * radius) - up.y * radius,
                    (center.z + right.z * radius) - up.z * radius,
                );
                let matrix = dereth_render::camera::projection(&view) * view.view;
                let project = |p| {
                    let p = localtoglobal(&object.frame, p);
                    let q = matrix * glam::Vec4::new(p.x, p.z, p.y, 1.0);
                    if q.w < 0.0002 {
                        return None;
                    }
                    // Retail's transform keeps homogeneous screen numerators as
                    // floats, then truncates the projected point at 1/128px,
                    // then WorldObjects truncates again into its RECT.
                    let xw = q.x * eye.width * 0.5 + eye.width * q.w * 0.5;
                    let yw = q.w * eye.height * 0.5 - q.y * eye.height * 0.5;
                    let pixel = |n: f32| {
                        let subpixel = dereth_primitives::num::to_i32_f64(
                            f64::from(n) * 128.0 / f64::from(q.w),
                        );
                        dereth_primitives::num::to_i32_f64(f64::from(subpixel) / 128.0)
                    };
                    Some((pixel(xw), pixel(yw)))
                };
                if let (Some(a), Some(b)) = (project(a), project(b)) {
                    return Some(Projection::OnScreen((a.0, a.1, b.0, b.1)));
                }
            }
            // The off-screen bearing is in CAMERA local space, not player heading.
            // Behind the camera retail discards elevation and chooses left or right.
            let mut local = globaltolocal(&camera, world_center);
            let heading = if dereth_physics::math::V3::normalize_check_small(&mut local) {
                0.0
            } else {
                let z = if local.y <= 0.0 { 0.0 } else { local.z };
                #[allow(clippy::cast_possible_truncation)]
                // Retail stores this extended-precision result into a float before its fmod call.
                let degrees = (450.0
                    - dereth_primitives::num::math::atan2(f64::from(z), f64::from(local.x))
                        * (180.0 / std::f64::consts::PI)) as f32;
                degrees % 360.0
            };
            Some(Projection::OffScreen(heading))
        }

        /// An upper bound on the bytes one frame pushes into `Gpu`'s dynamic upload arena: every
        /// cell drawing both triangles, plus 256 bytes each for the two constant buffers every
        /// `draw_dynamic` uploads, plus the object batches.
        ///
        /// This exists because the arena **grows by replacing its buffer and rewinding to zero**,
        /// which invalidates every address already recorded into the open command list. Reserving
        /// the whole frame's worth before any draw is recorded is the only way to keep that from
        /// happening mid-frame.
        fn worst_case_upload_bytes(&self) -> usize {
            const CB: usize = 256; // D3D12_CONSTANT_BUFFER_DATA_PLACEMENT_ALIGNMENT
            let align = |n: usize| n.div_ceil(CB) * CB;
            let stride = LAND_VERTEX_STRIDE as usize;
            let mut total = 0usize;
            for b in self.blocks.values() {
                let cells = b.cell_texture.len();
                total += cells * (align(6 * stride) + 2 * CB);
                for batch in b.opaque.iter().chain(&b.blended) {
                    // Deliberately the **whole** baked buffer and not this frame's
                    // assembled subset. The reservation is a worst case, and the worst case for a
                    // chunked batch is every placement back on level 0 -- which
                    // `degrades_disabled` makes happen the moment the map camera opens.
                    total += align(batch.vertices.len()) + 2 * CB;
                }
                // The indoor pass re-uploads a cell's mesh every frame the traversal
                // reaches it, exactly as the outdoor batches are re-uploaded. Only the cells the
                // portal traversal reaches are drawn, so this is the worst case and not the usual
                // one -- which is what the reservation wants.
                for m in b.env_cells.iter().flat_map(|c| &c.meshes) {
                    total += align(m.vertices.len()) + 2 * CB;
                }
                // And so are those cells' baked objects.
                for m in b
                    .env_cells
                    .iter()
                    .flat_map(|c| c.statics.iter().chain(&c.statics_blended))
                {
                    total += align(m.vertices.len()) + 2 * CB;
                }
            }
            // The character's parts are re-uploaded every frame, because they move every frame.
            // Every **level**, not only the one drawn. The reservation is a worst
            // case and the worst case is every part back at whichever level is largest; summing
            // the levels bounds that without having to know which, exactly as the chunked static
            // batches above reserve their whole pool.
            for m in self.character_parts.iter().flat_map(PartLevels::all) {
                total += align(m.vertices.len()) + 2 * CB;
            }
            // So are the server's objects.
            for o in self.object_draws.values() {
                for m in o.meshes.iter().flat_map(PartLevels::all) {
                    total += align(m.vertices.len()) + 2 * CB;
                }
            }
            // And so is the sky, both passes.
            if let Some(sky) = &self.sky {
                total += sky.upload_bytes();
            }
            total
        }

        /// The reservation [`Self::reserve_upload_arena`] makes.
        #[must_use]
        pub fn upload_reservation(&self) -> usize {
            // A little slack for a future pass (sky, a HUD) rather than a bound that is exactly
            // tight and fails the moment anything is added.
            self.stats.upload_bytes + 64 * 1024
        }

        /// Grow every ring slot's dynamic upload arena to [`Self::upload_reservation`] before any
        /// draw is recorded into it.
        ///
        /// A terrain frame crosses the initial 64 KiB arena within its first dozen draws.
        /// `dereth_render` retires an outgrown arena behind the fence, so skipping this is not a
        /// crash, but it is still worth doing: one allocation up front rather than a
        /// doubling chain every frame, and it keeps a frame's peak arena honest at ~4.5 MiB. Each
        /// iteration is an empty frame, the same three black frames the device flushes after a
        /// reset.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn reserve_upload_arena(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            let zeros = vec![0u8; self.upload_reservation()];
            for _ in 0..dereth_render::device::FRAME_COUNT {
                gpu.begin_frame()?;
                gpu.upload_bytes(&zeros)?;
                gpu.end_frame()?;
            }
            gpu.wait_idle()
        }

        /// Advance the character and the camera, then re-centre the landblock window on the
        /// viewer.
        ///
        /// The re-centre is the integer half only — the scroll, the released blocks and the new
        /// origins. The blocks it exposed are fetched by [`Self::stream`], which needs the device
        /// and therefore runs outside the frame bracket.
        ///
        /// `now` is current time, sampled once per frame by [`dereth_client_runtime::app::Clock`]. It reaches
        /// physics unchanged: the 30 Hz gate is inside the physics update and
        /// is **not** a frame-rate cap, so nothing here may pre-filter or quantise it.
        pub fn update(
            &mut self,
            ws: &mut WorldState,
            input: crate::camera::CameraInput,
            character: dereth_client_runtime::character::CharacterInput,
            now: dereth_primitives::LocalTime,
            dt: f32,
        ) {
            // The frame boundary for the two per-frame degrade counters. `update`
            // is and `stream` is the fetch that follows it (`App::frame`:
            // `world.update` then `stream_world`), so zeroing here and accumulating in
            // [`Self::refresh_degrade_levels`] makes both counters mean "this frame's work"
            // whether or not a block arrived.
            self.stats.degrade_billboard_turns = 0;
            self.stats.degrade_assemblies = 0;
            // Everything up to the re-centre is the world's simulation: object targets and
            // physics, the body's tick and its collision scripts, the chase camera's orbit.
            let physics_ticked =
                world_step::step_physics(ws, input, character, now, dt, &mut self.stats);
            // Viewer-cell updating is called from normal rendering with the
            // viewer's cell, after everything that could have moved it. `update_viewer_cell` is
            // its cell half and has to run *after* the re-centre, because the cell it computes is
            // an index into the viewer's own block.
            let space = self.recenter(ws);
            // This must stay on this side of `recenter`, **and the compiler checks
            // it**: `space` is minted by the line above and consumed by the drawn-frame writers
            // `step_objects` runs, so moving it up fails to compile rather than costing a
            // one-frame flicker.
            world_step::step_objects(ws, &self.cfg, space, now, physics_ticked, &mut self.stats);
            // Update the static light pool on a cell change,
            // the dynamic pool every frame, and the env-cell burn-in that
            // environment-cell drawing would run at draw. After `advance_objects`, because a
            // moving object's light follows its frame.
            self.update_world_lights(ws);
            // The physics time step runs the particle managers after the objects it
            // emits from have been stepped, so an emitter created by this frame's hooks is
            // updated in the same frame the client would have updated it.
            self.update_particles(ws, now);
            self.use_time_sky(ws, now.0, dt);
            // Adaptive degrade runs **once per frame at the end of normal rendering**,
            // which is the last line of this function, and is next -- so the bias the
            // frame draws with is the one this call just produced.
            self.calc_deg_level(dt);
            // Last, because it reads the bias `calc_deg_level` just produced and the
            // block origins `recenter` just moved.
            let switches = self.refresh_degrade_levels(ws);
            self.stats.degrade_switches += u64::from(switches);
            self.refresh_degrade_stats();
            // Immediately after: the same decision for everything that moves, and it
            // needs the same two predecessors -- the bias `calc_deg_level` just produced and the
            // part frames `advance_objects` just wrote.
            self.stats.part_degrade_switches += u64::from(self.refresh_part_levels(ws));
        }

        /// Update the frame-rate window, then update the adaptive degrade multiplier.
        ///
        /// **The two halves are one call here where the client has them a frame apart**, and that
        /// is not a shortcut: the client measures at frame end, at the *bottom* of
        /// frame *n*, and reads the result during world update, at the *top* of frame *n+1*, so
        /// what [`Self::calc_deg_level`] sees is a window ending with the duration of the previous frame.
        /// `dt` here is `time(n) - time(n-1)`, which is exactly that
        /// duration, so pushing it at the top of frame *n* leaves the ring holding the same twenty
        /// numbers the client's last-frame-times ring holds at the same moment.
        ///
        /// **The headless gate.** The whole of the loop's input is this `dt`, and this
        /// `dt` is a difference of frame timestamps, which `dereth_client_runtime::app::Clock::fixed_step` makes
        /// a fixed quantum under `--headless`. So a headless frame measures exactly
        /// `20 / (k * HEADLESS_STEP)` for the *k*th frame on every machine, the loop's trajectory
        /// is a function of the frame **count** and not of elapsed time, and the capture stays
        /// byte-identical across runs whether the governor is pinned or live. Nothing in this path
        /// reads a wall clock: the wall clock is the network's clock and is not consulted
        /// here. (`SkyScene::use_time`'s UV scroll takes the same `dt` for the same reason, so the
        /// two subsystems that are functions of elapsed time are functions of the *same* one.)
        fn calc_deg_level(&mut self, dt: f32) {
            self.degrade.frame_rate.push(f64::from(dt));
            let fps = self.degrade.frame_rate.fps();
            self.degrade.governor.use_time(fps);
            self.degrade.frames += 1;
        }

        /// For every baked static
        /// placement in the window, then re-assemble the batches whose choice changed.
        ///
        /// The client runs this per part per frame and issues one draw per part; this build
        /// batches by surface, so the same decision is made per part and the *assembly* is what
        /// costs. It is deliberately not a per-frame rebake: the level a placement draws changes
        /// only when it crosses a threshold, so the assembled buffers are rebuilt on the frames
        /// that cross one and reused on every frame in between. A walk through Holtburg changes a
        /// double-digit number of levels per frame out of thousands of placements.
        ///
        /// It must run **after** [`Self::calc_deg_level`], because the bias that frame's
        /// thresholds slide by is the one the loop just produced, and after the re-centre, because
        /// `block.origin` moves with it.
        fn refresh_degrade_levels(&mut self, ws: &mut WorldState) -> u32 {
            let globals = self.degrade_globals(ws);
            let cam = detail_viewer(ws);
            // Every baked placement belongs to an ordinary object, never a particle emitter.
            let share_2dsq = self.degrade.governor.level().share_distance_2dsq(false);
            let mut switches = 0u32;
            let mut turns = 0u32;
            let mut assemblies = 0u32;
            for block in self.blocks.values_mut() {
                let viewer = Vec3::new(cam.x - block.origin.0, cam.y - block.origin.1, cam.z);
                let first = !block.degrade_assembled;
                let (changed, n, t) =
                    select_levels(&mut block.degrade, true, viewer, share_2dsq, &globals);
                switches += n;
                turns += t;
                if changed || first {
                    assemble_batches(&mut block.opaque, &block.degrade);
                    assemble_batches(&mut block.blended, &block.degrade);
                    assemblies += 1;
                }
                for cell in &mut block.env_cells {
                    let (changed, n, t) =
                        select_levels(&mut cell.degrade, false, viewer, share_2dsq, &globals);
                    switches += n;
                    turns += t;
                    if changed || first {
                        assemble_batches(&mut cell.statics, &cell.degrade);
                        assemble_batches(&mut cell.statics_blended, &cell.degrade);
                        assemblies += 1;
                    }
                }
                block.degrade_assembled = true;
            }
            // A billboard turn is a reassembly the way a level change is, so the two
            // costs are reported side by side rather than folded together: `degrade_switches`
            // counts levels that changed and these two count the turns and reassemblies.
            //
            // **They accumulate, and [`Self::update`] zeroes them at the top of the frame**, because
            // this function runs **twice** on a frame that also streams -- once from `update` and
            // once from `stream`, for the reason recorded at the second call site. Assigning
            // would let `stream`'s second pass (which finds nothing changed, correctly) overwrite
            // the real count with 0 on exactly the frames a block arrives.
            self.stats.degrade_billboard_turns += turns as usize;
            self.stats.degrade_assemblies += assemblies as usize;
            switches
        }

        /// Pick this frame's degrade level for every part of every
        /// thing that moves: the server's objects and the local body.
        ///
        /// For each part, the viewer-distance update computes the distance and the viewer heading.
        /// A degrading part of an object other than the player then takes its level and mode from
        /// the degrade lookup at that distance divided by the graphics-object scale's z; any other
        /// part uses level 0. The draw frame is calculated from the mode and the heading, and the
        /// draw stops if the selected level's graphics object is missing.
        ///
        /// This is `dereth_world_render::objects::parts::select_level`'s production caller, with
        /// the scale divide and the player exemption in it.
        ///
        /// It must run **after** the object step ([`dereth_client_runtime::world_step::step_objects`]), which is what writes `part.pos` from
        /// this frame's animation frame, and after [`Self::calc_deg_level`], whose bias this
        /// frame's thresholds slide by — the same two orderings
        /// [`Self::refresh_degrade_levels`] has, for the same two reasons.
        ///
        /// **The billboarding mode is kept too.**
        /// `select_level` fills `PartDraw::draw_pos` with the draw frame calculated from the part
        /// position, the degrade mode and the viewer heading.
        /// It is stored per part here and submitted by `draw_part`, gated on
        /// [`SceneConfig::part_billboards`] so that submitting `part.pos` instead is a control arm.
        fn refresh_part_levels(&mut self, ws: &mut WorldState) -> u32 {
            let globals = self.degrade_globals(ws);
            let billboards = self.cfg.part_degrade_levels && self.cfg.part_billboards;
            let cam = detail_viewer(ws);
            let share = self.degrade.governor.level();
            let mut switches = 0u32;
            let mut resident = 0usize;
            let mut drawn_tris = 0usize;
            let mut drawn_batches = 0usize;
            let (mut with_record, mut degraded, mut culled) = (0usize, 0usize, 0usize);
            // The same "asked / answered / acted on" three-state shape the degrade
            // counters have: how many parts' selected level asks for a billboarding mode, and how
            // many of those actually got a different frame.
            let (mut part_billboards, mut part_turns) = (0usize, 0usize);

            for (id, o) in &ws.objects {
                let shared = object_shared_distance(ws, o, cam, &share);
                let d = self
                    .object_draws
                    .get_mut(id)
                    .expect("every world object has its drawing half");
                d.part_levels.resize(d.meshes.len(), 0);
                d.part_draw_pos.resize(d.meshes.len(), Frame::default());
                d.part_cypt.resize(d.meshes.len(), 0.0);
                for (i, pl) in d.meshes.iter().enumerate() {
                    resident += pl.triangles_held();
                    if pl.info.is_some() {
                        with_record += 1;
                    }
                    let driver = o.sim.driver.borrow();
                    let Some(part) = driver.part_array.parts.get(i) else {
                        continue;
                    };
                    let chosen = part_level(pl, part, o.is_player, cam, shared, &globals);
                    let (level, mode, draw_pos) = (chosen.level, chosen.mode, chosen.draw_pos);
                    // The fourth thing kept out of the one viewer-distance update
                    // this function already runs. Unconditional: the sort key is measured whether
                    // or not `part_depth_sort` is set, so that the control arm and the live arm
                    // differ only in whether `draw` *uses* it.
                    d.part_cypt[i] = chosen.cypt;
                    // With the switch clear this is `part.pos`, the unbillboarded frame.
                    d.part_draw_pos[i] = if billboards { draw_pos } else { part.pos };
                    if mode != dereth_world_render::objects::degrade::DegradeMode::None {
                        part_billboards += 1;
                        if d.part_draw_pos[i] != part.pos {
                            part_turns += 1;
                        }
                    }
                    if d.part_levels[i] != level {
                        d.part_levels[i] = level;
                        switches += 1;
                    }
                    if level != 0 {
                        degraded += 1;
                        if pl.at(level).is_empty() {
                            culled += 1;
                        }
                    }
                    if !o.drawn || part.no_draw() {
                        continue;
                    }
                    drawn_tris += pl.triangles_at(level);
                    drawn_batches += pl.at(level).len();
                }
            }
            self.stats.server_object_triangles = drawn_tris;
            self.stats.server_object_batches = drawn_batches;
            self.stats.server_object_triangles_resident = resident;

            let (mut body_tris, mut body_batches, mut body_parts, mut body_resident) =
                (0usize, 0usize, 0usize, 0usize);
            if let Some(c) = ws.character.as_ref() {
                let shared = self.character_shared_distance(ws, cam, &share);
                let driver = c.driver();
                self.character_part_levels
                    .resize(self.character_parts.len(), 0);
                self.character_part_draw_pos
                    .resize(self.character_parts.len(), Frame::default());
                self.character_part_cypt
                    .resize(self.character_parts.len(), 0.0);
                for (i, pl) in self.character_parts.iter().enumerate() {
                    body_resident += pl.triangles_held();
                    if pl.info.is_some() {
                        with_record += 1;
                    }
                    let Some(part) = driver.part_array.parts.get(i) else {
                        continue;
                    };
                    // `true`: this is the local player, the one object the viewer-distance update
                    // exempts. Every level is on the device; nothing but this flag keeps him at 0.
                    let chosen = part_level(pl, part, true, cam, shared, &globals);
                    let (level, mode, draw_pos) = (chosen.level, chosen.mode, chosen.draw_pos);
                    // The player is exempt from *degrading*, not from being
                    // sorted: the viewer-distance update measures the distance before it tests
                    // whether the object is the player.
                    self.character_part_cypt[i] = chosen.cypt;
                    // The player is exempt from *degrading*, not from *billboarding*:
                    // `select_level` short-circuits to level 0 and reads level 0's own mode, so a
                    // body part whose level-0 record says mode 5 still turns. That is the client's
                    // own arithmetic and it is why this is not hard-coded to `part.pos`.
                    self.character_part_draw_pos[i] = if billboards { draw_pos } else { part.pos };
                    if mode != dereth_world_render::objects::degrade::DegradeMode::None {
                        part_billboards += 1;
                        if self.character_part_draw_pos[i] != part.pos {
                            part_turns += 1;
                        }
                    }
                    if self.character_part_levels[i] != level {
                        self.character_part_levels[i] = level;
                        switches += 1;
                    }
                    if level != 0 {
                        degraded += 1;
                    }
                    if part.no_draw() {
                        continue;
                    }
                    let t = pl.triangles_at(level);
                    body_tris += t;
                    body_batches += pl.at(level).len();
                    if t > 0 {
                        body_parts += 1;
                    }
                }
            }
            self.stats.character_triangles = body_tris;
            self.stats.character_batches = body_batches;
            self.stats.character_parts = body_parts;
            self.stats.character_triangles_resident = body_resident;

            self.stats.part_degrade_parts = with_record;
            self.stats.part_degrade_parts_degraded = degraded;
            self.stats.part_degrade_parts_culled = culled;
            // **After both loops**: these two are summed over the server's objects and
            // the local body, and publishing them between the two would silently drop the body's half.
            self.stats.part_billboards = part_billboards;
            self.stats.part_billboards_turned = part_turns;
            switches
        }

        /// The current degrade inputs read this frame.
        ///
        /// The bias is `deg_mul` when automatic degrades are on and the user-supplied degrade bias otherwise, so a **pinned**
        /// governor is expressed the way the client expresses one: automatic degrades off
        /// and the pinned value standing in as the user-supplied bias. With
        /// [`dereth_world_render::consts::PINNED_DEG_MUL`] at 0 the two spellings agree, and the
        /// point of writing it out is that they keep agreeing if it ever moves.
        #[must_use]
        pub fn degrade_globals(
            &self,
            ws: &WorldState,
        ) -> dereth_world_render::objects::degrade::DegradeGlobals {
            let g = &self.degrade.governor;
            dereth_world_render::objects::degrade::DegradeGlobals {
                degrades_disabled: ws.degrades_disabled(),
                // -1 in every running frame, because retail has
                // no writer for it and neither does this build -- see [`Self::set_force_level`].
                force_level: self.force_level,
                auto_update_deg_mul: g.auto,
                deg_mul: g.deg_mul,
                // The pinned degrade multiplier comes from
                // `Render.GraphicsPerformance`, which `get_degrade` reads when
                // `auto_update_deg_mul` is clear. With the governor pinned the two agree at
                // `PINNED_DEG_MUL`; a profile that names the preference moves it, which is the
                // whole of what the slider is for. A pinned governor at a *non*-default
                // `PINNED_DEG_MUL` still wins, because that is this build's determinism
                // decision and not a user setting.
                user_bias: if g.auto {
                    0.0
                } else if g.deg_mul == dereth_world_render::consts::PINNED_DEG_MUL {
                    self.cfg.render.graphics_performance
                } else {
                    g.deg_mul
                },
                // The degrade-distance preference -- `Render.DegradeDistance`, whose
                // initial value is the 50.0 `DegradeGlobals::default()` carried -- on every
                // world, the 2005 one included (CD-026).
                degrade_distance: self.cfg.render.degrade_distance,
            }
        }

        /// The landscape time update's environment and lighting tick pair.
        ///
        /// The clock update runs every frame, before the gate. Once `cur_time >= next_tick`,
        /// `next_tick` becomes `cur_time` plus the sky's tick size; if also
        /// `cur_time > next_light_tick`, the lighting is read and applied to the landscape and
        /// `next_light_tick` becomes `cur_time` plus the sky's light tick size; then the fog is
        /// updated.
        ///
        /// The fog arm is reproduced through [`Self::apply_fog`] and the renderer's
        /// linear-fog constants, with the shared admin override transition applied in the same
        /// consumer.
        fn use_time_sky(&mut self, ws: &mut WorldState, now: f64, dt: f32) {
            world_step::advance_clock(ws, now);
            self.update_sky(ws, dt);
            let Some(light_tick) = world_step::tick_schedule(
                ws,
                self.sky_region.as_deref().unwrap_or(&self.land.region),
                now,
            ) else {
                return;
            };
            if light_tick && self.apply_lighting(ws) {
                self.relight_blocks(ws);
            }
            // The fog is on the *outer* tick, not the light tick:
            // falls through the `next_light_tick` branch into fog application every `tick_size`.
            self.apply_fog(ws);
        }

        /// The landscape weather-enable flag, whose default is 1. A test
        /// needs it because the one honest way to say "these pixels are the weather layer" is to
        /// draw the same frame with the layer suppressed: sky-object creation makes
        /// no object at all for a `properties & 4` entry while it is clear, so the differential is
        /// exactly the weather.
        ///
        /// The stored weather-enable byte has only two
        /// callers: initial scene setup and the character-option handler, both passing
        /// the inverse of the `DisableMostWeatherEffects` option. Here
        /// `OptionSideEffect::EnableWeather` names the call; see
        /// the client shell's `App::apply_player_option_effects`.
        pub fn set_weather_enabled(&mut self, on: bool) {
            if let Some(sky) = self.sky.as_mut() {
                sky.weather_enabled = on;
            }
        }

        /// The landscape weather-enable flag as it stands — the read side of
        /// [`Self::set_weather_enabled`], so a station can say *"the weather layer is off"* rather
        /// than *"a function was called"*. `None` before the sky exists.
        #[must_use]
        pub fn weather_enabled(&self) -> Option<bool> {
            self.sky.as_ref().map(|s| s.weather_enabled)
        }

        /// The landscape fog-enable flag is the `DisableDistanceFog` character option's whole
        /// effect.
        ///
        /// Initial option setup and option-change handling both compute the inverse of the
        /// `DisableDistanceFog` option.
        /// The result is stored straight into the landscape flag.
        /// There is no third state and no deferred option transaction.
        /// The landscape tick reads that same byte.
        /// When it is clear, the tick disables fixed-function fog.
        /// It then skips world-fog lookup and fog-property installation.
        /// When it is set, those normal fog steps remain enabled.
        /// That is why this setter changes the existing `SceneConfig::world_fog` consumer rather
        /// than adding a separate render switch, and it
        /// skips the whole world-fog query and state update when it is clear, which is
        /// what [`Self::world_fog_state`] already models off `SceneConfig::world_fog`. This is the
        /// setter that lets the character option reach a field that is otherwise a start-up
        /// switch.
        ///
        /// [`Self::apply_fog`] is re-run here rather than waiting for the next landscape tick,
        /// because retail's immediate tick reset does not exist on this path and a
        /// player who unticks the row expects the fog to go within the frame.
        pub fn set_world_fog(&mut self, ws: &mut WorldState, on: bool) -> bool {
            if self.cfg.world_fog == on {
                return false;
            }
            self.cfg.world_fog = on;
            self.sync_environment_override_flags(ws);
            self.apply_fog(ws)
        }

        /// Attach the process-owned environment-override globals to a newly constructed landscape.
        /// The handle includes the current transition, so this never restarts an in-flight blend.
        pub(crate) fn set_environment_override_state(
            &mut self,
            ws: &mut WorldState,
            state: EnvironmentOverrideState,
        ) {
            ws.environment_override = state;
            self.sync_environment_override_flags(ws);
        }

        /// Refresh the two sky-pass predicates after the shared Admin state changes.
        pub(crate) fn sync_environment_override_flags(&mut self, ws: &mut WorldState) {
            let enabled = ws.environment_override.snapshot().enabled;
            if let Some(sky) = self.sky.as_mut() {
                sky.override_enabled = enabled;
                sky.fog_enabled = enabled && self.cfg.world_fog;
            }
        }

        /// The always-daylight flag is the `PersistentAtDay` character option's
        /// whole effect.
        ///
        /// In the original client, enabling it stores a normalized boolean.
        /// The operation then clears the low and high halves of the next general landscape tick
        /// and the low and high halves of the next lighting tick.
        /// Those are its five stores: one flag byte and four timer words.
        /// It replaces no sky object,
        /// enables or disables no weather object,
        /// and changes neither the game clock
        /// nor the current day and year.
        /// Existing landscape colors remain until lighting is evaluated again;
        /// clearing both timers makes that evaluation happen on the next frame.
        /// The ordinary arm continues to use the caller's current time-of-day lighting,
        /// while the always-daylight arm ignores those four lighting values
        /// and queries the region again at time of day `0.5`.
        /// Both arms then apply the same minimum-ambient clamp,
        /// so the option affects landscape lighting only.
        /// Its observable value is noon lighting, not a frozen world clock;
        /// disabling it returns the next evaluation to the live time of day.
        /// The timer reset makes either transition visible without waiting for a scheduled tick.
        ///
        /// In other words, the flag plus a forced re-tick makes the change visible on the next frame rather
        /// than at the next 0.5 s landscape tick. The flag has exactly **two** readers: the `/day`
        /// console command, which toggles it, and landscape lighting, whose always-daylight arm
        /// **throws away the caller's four arguments** and re-asks the region for the lighting at
        /// time-of-day `0.5`, followed by the same minimum-ambient clamp as the normal arm.
        ///
        /// So *Always Day* is **noon landscape lighting**, and nothing else: it does not touch the
        /// sky dome, weather or clock. This Rust setter stores `always_daylight` and immediately
        /// calls [`Self::apply_lighting`], rather than writing the original client's timer words;
        /// the immediate update is equivalent to forcing the next lighting evaluation.
        pub fn set_always_daylight(&mut self, ws: &mut WorldState, on: bool) -> bool {
            if ws.always_daylight == on {
                return false;
            }
            ws.always_daylight = on;
            // Equivalent to the original client's timer reset: re-light now rather than waiting
            // for the next landscape tick -- and, as that tick does, re-light the resident blocks
            // too. The landscape's lighting is baked into each block's vertex colours when the
            // block is built, so storing the new values alone re-lit only blocks built after the
            // switch; and because the stored values then already matched, the next tick found
            // nothing to do either, and the blocks on screen kept the old hour's light for good.
            let changed = self.apply_lighting(ws);
            if changed {
                self.relight_blocks(ws);
            }
            changed
        }

        /// The baked vertex colours of the block the viewer stands in, for the tests: what the
        /// landscape lighting has actually been applied to, as opposed to what it is set to.
        #[must_use]
        pub fn viewer_block_colours(&self, ws: &WorldState) -> Option<Vec<[u8; 3]>> {
            let r = ws.streamer.window.mid_radius();
            ws.streamer
                .window
                .slot(r, r)?
                .mesh
                .as_ref()
                .and_then(SlotMesh::get::<LandblockMesh>)
                .map(|m| m.colours.clone())
        }

        /// Return `sunlight` and `ambient_level` as they stand, for the tests. The vector
        /// is unnormalised and its length is the brightness.
        #[must_use]
        pub fn landscape_lighting(&self) -> LandscapeLighting {
            self.land.lighting
        }

        // The per-object physics / animation step lives in
        // [`dereth_client_runtime::object_step`]: `refresh_object_env` .. `pull_stuck_objects` and the
        // `server_object_*` motion probes. The scene keeps the render half of a `SceneObject`
        // and hands the step the simulation half; these are the call sites, and the four
        // counters the step keeps come back as an `ObjectStepStats` folded into `SceneStats`.

        fn fold_step_stats(&mut self, d: dereth_client_runtime::object_step::ObjectStepStats) {
            world_step::StepStatsSink::fold_steps(&mut self.stats, d);
        }

        /// The tail of the camera update — **the body fades as the chase camera
        /// closes on it.**
        ///
        /// * With the camera in the head, the latched value becomes 1.0 and the player's whole
        ///   hierarchy is set to translucency 1.0.
        /// * With no pivot position, a latched value above 0 is reset to 0 and applied.
        /// * With the pivot within 0.45 of the viewer, the value is
        ///   `clamp(1 - (0.2 - d) / -0.24999999, 0, 1)` and is applied to the whole hierarchy.
        /// * Otherwise a latched value above 0 is reset to 0 and applied.
        ///
        /// [`crate::camera::CameraControl::player_translucency`] is that value; applying it is the
        /// whole of the wire.
        ///
        /// **The latch is load-bearing, not an optimisation.** The client only issues the `0.0`
        /// call on the *transition*, and clears the
        /// `NoDraw` bit on every call with `t != 1.0`. Issuing `0.0` unconditionally would
        /// therefore make the chase camera un-hide any part an animation's `NoDraw` hook had
        /// hidden, every frame. So: apply whenever the value is non-zero, and apply a zero only
        /// once, exactly as the three branches above do.
        ///
        /// **Hierarchical** means the operation
        /// recurses into `children` — so a wielded weapon fades with the hand holding it. The
        /// player's children here are the held objects attached to him.
        pub fn apply_camera_translucency(&mut self, ws: &mut WorldState) {
            world_step::apply_camera_translucency(ws, &self.cfg);
        }

        /// The landscape half of the position change `(pos, 1)` during player teleport.
        ///
        /// After the position change's same-cell-and-live-cell fast path, the teleport arm calls
        /// whole-landscape release regardless of how near the destination
        /// is. A normal [`Self::recenter`] deliberately retains the blocks shared by the old and
        /// new windows, so it cannot stand in for that edge. Move every resident draw through the
        /// existing release queues, hand back every visible cell, and recreate the same-radius
        /// window at the position the accepted packet named.
        /// [`Self::stream`] performs the device-resource release and full arrivals later in this
        /// frame, after [`Self::sync_objects`] has run the queued
        /// half.
        pub(crate) fn release_landscape_for_teleport(
            &mut self,
            ws: &mut WorldState,
            destination: dereth_primitives::LandblockId,
        ) {
            let departed: Vec<_> = self.blocks.keys().copied().collect();
            self.pending.clear();
            for block in &departed {
                if let Some(draw) = self.blocks.remove(block) {
                    self.released_blocks.push(draw);
                }
            }
            self.release_block_interiors(ws, &departed);
            self.static_pool_key = None;

            let radius = ws.streamer.window.mid_radius();
            let was = ws.streamer.window.viewer_block();
            ws.streamer.window = LandblockWindow::new(radius);
            let viewer = (i32::from(destination.x()), i32::from(destination.y()));
            if let Some(was) = was {
                world_step::rebase_render_space_particles(ws, (viewer.0 - was.0, viewer.1 - was.1));
            }
            let actions = ws.streamer.window.update_block(viewer);
            self.queue(ws, &actions);
        }

        /// The [`ViewParams`] the renderer turns into matrices, for the current back-buffer size.
        #[must_use]
        pub fn view_params(&self, ws: &WorldState, width: u32, height: u32) -> ViewParams {
            // The 3D viewport is the world-view element's rectangle, not the
            // window's. [`Self::set_game_viewport`] is the one writer; `None` is the whole back
            // buffer, which is what a headless scene with no UI shell has.
            let viewport = self
                .game_viewport
                .unwrap_or(dereth_render::camera::Viewport {
                    x: 0,
                    y: 0,
                    width,
                    height,
                });
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent, at most a few thousand
            let (fw, fh) = (width as f32, height as f32);
            // The display aspect ratio is the display-aspect calculation's result,
            // and that reads the **display**, not the viewport: it is the pref (4/3 or 16/9) or
            // the back buffer's own ratio. So the two arguments are different rectangles and
            // collapsing them would make the FOV follow the window instead of the view.
            // `Render.AspectRatio`, which was pinned at `Normal` here and read no
            // preference. The render-preference poll is its one writer and calls the calculation
            // whenever the preference's shadow copy changes.
            let display_aspect = self.cfg.render.aspect().display_aspect_ratio(fw, fh);
            // Viewport aspect calculation, called by the viewport setter
            // with the rectangle it was just handed:
            // `(w / h) * display_aspect_ratio * 0.75` for the non-raw case, which is the value
            // both the clamped-position recalculation and the game-view update notice
            // pass. **The width and height here are the viewport's**, which is the whole of what
            // makes a shrunken smart box show a narrower view rather than a squashed one.
            #[allow(clippy::cast_precision_loss)]
            let (vw, vh) = (viewport.width.max(1) as f32, viewport.height.max(1) as f32);
            let aspect =
                dereth_render::camera::compute_aspect_for_viewport(vw, vh, display_aspect, false);
            ViewParams {
                view: dereth_render::camera::view_from_frame(&ws.camera.frame()),
                // The game field of view, which default-FOV setup
                // builds as `Render.FieldOfView * 0.017453292` and the render-preference
                // poll re-applies whenever its shadow copy changes.
                // It was `DEFAULT_FOV_DEGREES` here and read no preference.
                fov_y_rad: dereth_render::camera::fov_y_from_preference(
                    self.cfg.render.game_fov_rad(),
                    aspect,
                ),
                aspect,
                viewport,
                // `D3DRS_AMBIENT`, as the light update quantises the world
                // ambient color on every viewpoint update.
                lights: dereth_render::LightBlock {
                    ambient: ambient_render_state(self.world_ambient_color(ws)),
                    ..dereth_render::LightBlock::default()
                },
                // `D3DRS_FOGENABLE` / `FOGCOLOR` / `FOGSTART` / `FOGEND`, as
                // the landscape time update left them on the device, not `FogParams::default()`
                // -- the default device states' grey 400..2000 with fog *off*.
                fog: self.fog,
                ..ViewParams::default()
            }
        }

        /// Install the 3D viewport's rectangle through the render device's
        /// viewport update, as reached from the viewport-size notice handler.
        ///
        /// `None` restores the whole back buffer, which is `dereth_render_cpu::camera::compute_game_viewport`'s own
        /// answer when nothing is docked and is what a scene with no UI shell wants.
        ///
        /// The rectangle is not clamped here: the client's viewport update clamps against
        /// the render-target width and height and so does [`dereth_render::camera::clamp_viewport`],
        /// which `Gpu::set_viewport` runs. Clamping twice, against two different extents, is how a
        /// letterbox becomes a one-pixel strip.
        pub fn set_game_viewport(&mut self, viewport: Option<dereth_render::camera::Viewport>) {
            self.game_viewport = viewport;
        }

        /// The rectangle the scene **actually draws into** at this back-buffer size.
        ///
        /// Deliberately not a getter for [`Self::game_viewport`]: it is
        /// `view_params(width, height).viewport`, i.e. the value that reaches
        /// `Gpu::set_viewport` and `ViewParams`, so a build that stored the rect and then ignored
        /// it answers the window here. That distinction is not academic -- a test that read the
        /// stored field would stay green against exactly that mutation.
        #[must_use]
        pub fn effective_viewport(
            &self,
            ws: &WorldState,
            width: u32,
            height: u32,
        ) -> dereth_render::camera::Viewport {
            self.view_params(ws, width, height).viewport
        }

        /// The viewer-space position and the two graphics-state matrices,
        /// gathered as one value.
        ///
        /// Viewpoint updating writes the viewer-space origin, while screen-to-view conversion reads
        /// the projection's first two diagonal values and the inverse world-to-view matrix. All
        /// three are globals there and are gathered here, from
        /// the **same** [`Self::view_params`] the frame is drawn with — which is the property that
        /// matters, because a cone built from a different projection is a cone of somewhere the
        /// player is not.
        ///
        /// `glam` stores columns and D3D stores rows, and
        /// [`dereth_render::camera::view_from_frame`] deliberately puts the D3D rows in the glam
        /// columns, so `to_cols_array()` **is** D3D row-major order and no transpose is needed
        /// here. The same is true of [`dereth_render::camera::perspective_fov_lh`], whose
        /// `to_cols_array()[0]` and `[5]` are `_11` and `_22`.
        fn eye_transform(
            &self,
            ws: &WorldState,
            view: &dereth_render::camera::ViewParams,
        ) -> dereth_world_render::cells::clip::EyeTransform {
            let proj = dereth_render::camera::projection(view).to_cols_array();
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent
            let (w, h) = (view.viewport.width as f32, view.viewport.height as f32);
            dereth_world_render::cells::clip::EyeTransform {
                viewpoint: ws.camera.frame().origin,
                inv_view: view.view.inverse().to_cols_array(),
                proj_11: proj[0],
                proj_22: proj[5],
                width: w,
                height: h,
            }
        }

        /// The viewer-space cone's `CY`, its first plane.
        ///
        /// The viewpoint update's Y axis is the viewer frame's forward column, which is exactly what
        /// [`dereth_world_render::math::get_vector_heading`] answers, so the camera's forward vector
        /// has one producer here as it does there.
        fn viewer_near_plane(&self, ws: &WorldState) -> dereth_world_render::Plane {
            let f = ws.camera.frame();
            dereth_world_render::cells::cull::viewer_near_plane(
                f.origin,
                dereth_world_render::math::get_vector_heading(&f),
                dereth_render::camera::ZNEAR,
            )
        }

        /// The inputs to [`Self::object_light_set`] for one part submission: the drawing sphere
        /// view-cone checking leaves as the local object centre and radius
        /// (the level's own sphere, scaled and placed by `draw_pos` as [`Self::part_cone`] places
        /// it), and the pass the object's cell puts it in.
        fn submission_light_set(&self, s: &PartSubmission<'_>) -> Vec<D3dLight> {
            use dereth_world_render::cells::cull::object_scale;
            let (centre, radius) = match s.drawing_sphere {
                Some((c, r)) => {
                    let scale = object_scale(s.part.gfxobj_scale);
                    let scaled = Vec3::new(c.x * scale, c.y * scale, c.z * scale);
                    (
                        dereth_world_render::math::localtoglobal(&s.draw_pos, scaled),
                        scale * r,
                    )
                }
                None => (s.draw_pos.origin, 0.0),
            };
            self.object_light_set(centre, radius, s.outdoors)
        }

        /// The view-cone test's answer for one submitted mesh part.
        ///
        /// The client's sequence, per part, in order:
        ///
        /// 1. The object scale is the largest axis of the part's graphics-object scale.
        /// 2. The part's draw position is pushed as the current frame (position level 1).
        /// 3. Mesh drawing draws the part's graphics object at its degrade level at that draw
        ///    position, and the view-cone check on the object's drawing sphere scales the centre
        ///    by the object scale, takes it to world space through the current frame, scales the
        ///    radius the same way, and tests the result against the viewer's world-space cone
        ///    plane and then each portal vertex plane.
        ///
        /// `draw_pos` and not `pos`: the draw call is handed the part's draw position (its placed
        /// position after the billboard adjustment), not the placed position itself. That is the
        /// same quantity
        /// [`PartSubmission::draw_pos`] carries, so the cone tests the pose the mesh is drawn at
        /// rather than the pose it was placed at.
        ///
        /// The scale is a **scalar**, the largest axis, and it multiplies the centre as well as the
        /// radius — see [`dereth_world_render::cells::cull::object_scale`].
        ///
        /// A part with no drawing sphere is counted and **not** culled; see
        /// [`ObjectConeStats::no_sphere`] for why that state cannot arise in retail at all.
        fn part_cone(
            &self,
            s: &PartSubmission<'_>,
            near: &dereth_world_render::Plane,
            view: &dereth_world_render::cells::clip::ViewPoly,
            stats: &mut ObjectConeStats,
        ) -> dereth_world_render::objects::draw::MeshDrawStatus {
            use dereth_world_render::cells::cull::{object_scale, viewcone_check, Bounding};
            use dereth_world_render::objects::draw::{draw_mesh_no_portal_list, MeshDrawStatus};

            let Some((centre, radius)) = s.drawing_sphere else {
                stats.no_sphere += 1;
                return MeshDrawStatus::InsideViewcone;
            };
            let scale = object_scale(s.part.gfxobj_scale);
            let scaled = Vec3::new(centre.x * scale, centre.y * scale, centre.z * scale);
            let world = dereth_world_render::math::localtoglobal(&s.draw_pos, scaled);
            let cone = viewcone_check(world, scale * radius, near, &view.planes);
            stats.tested += 1;
            if cone == Bounding::Outside {
                stats.outside += 1;
                if self.cfg.object_viewcone {
                    stats.culled += 1;
                }
            }
            // The portals-only flag is false on this path: the building portal pass is
            // the scene's building-portal draw, and it draws cells rather than object parts.
            draw_mesh_no_portal_list(cone, false)
        }

        /// The **non-null portal-list** branch, for
        /// the outdoor object pass an indoor frame issues under its outside-view list.
        ///
        /// A portal list with no views answers OUTSIDE. Otherwise, for each enabled view, the view
        /// is installed; when [`viewcone_check`](dereth_world_render::cells::cull::viewcone_check)
        /// answers OUTSIDE for the mesh the view is skipped, and otherwise the mesh is offered to
        /// the selection ray and drawn.
        ///
        /// The answers are combined by [`dereth_world_render::objects::draw::draw_mesh_view_list`],
        /// whose tail is the part that cannot be guessed: a mesh every view rejected answers
        /// `OutsideViewcone`, and an **empty** view list takes the same path. So "the
        /// window is not on screen" and "nothing is outdoors" are the same answer here, which is
        /// what makes the wall case and the sealed case one rule.
        ///
        /// `ObjectConeStats` counts one `tested` per part, not per view, so the numbers stay
        /// comparable with [`Self::part_cone`]'s across a build that changes which arm runs.
        fn part_cone_views(
            &self,
            s: &PartSubmission<'_>,
            near: &dereth_world_render::Plane,
            views: &[dereth_world_render::cells::clip::ViewPoly],
            stats: &mut ObjectConeStats,
        ) -> dereth_world_render::objects::draw::MeshDrawStatus {
            use dereth_world_render::cells::cull::{object_scale, viewcone_check, Bounding};
            use dereth_world_render::objects::draw::{draw_mesh_view_list, MeshDrawStatus};

            let Some((centre, radius)) = s.drawing_sphere else {
                stats.no_sphere += 1;
                return MeshDrawStatus::InsideViewcone;
            };
            let scale = object_scale(s.part.gfxobj_scale);
            let scaled = Vec3::new(centre.x * scale, centre.y * scale, centre.z * scale);
            let world = dereth_world_render::math::localtoglobal(&s.draw_pos, scaled);
            let answers: Vec<Bounding> = views
                .iter()
                .map(|v| viewcone_check(world, scale * radius, near, &v.planes))
                .collect();
            stats.tested += 1;
            let status = draw_mesh_view_list(&answers, false);
            if status == MeshDrawStatus::OutsideViewcone {
                stats.outside += 1;
                if self.cfg.object_viewcone {
                    stats.culled += 1;
                }
            }
            status
        }

        /// The whole function writes `id` into the view-cone check object id.
        ///
        /// Its one caller in the client is the tail of the selected-item notice handler;
        /// here it is driven from the native visibility latch, which
        /// that function's transcription writes, so the id the draw matches against is the
        /// registrant's and not the world's selected-object field.
        pub fn set_selected_object_id(&self, id: Option<ObjectId>) {
            self.viewcone_check_object_id.set(id.map_or(0, |i| i.0));
        }

        /// Whether [`Self::draw`] submitted a part of [`Self::set_selected_object_id`]'s object
        /// since this was last asked, clearing the observation.
        ///
        /// The *latch* is the native visibility state; this is one frame's
        /// part-drawing visibility result, handed across the crate seam.
        pub fn take_selected_part_drawn(&self) -> bool {
            self.selected_part_drawn.replace(false)
        }

        /// Reset the pick observation on the render side of the seam:
        /// a click that starts a pick throws away an observation this frame's `use_time` has
        /// not collected yet, exactly as it throws away the latch.
        pub fn clear_selected_part_drawn(&self) {
            self.selected_part_drawn.set(false);
        }

        /// The viewer position used for lighting. The client copies the player's
        /// position into it every frame, and light insertion
        /// measures every light's sort key from its origin. The cell id and the frame in render
        /// space; the free camera when there is no body.
        fn player_origin(&self, ws: &WorldState) -> (u32, Frame) {
            match ws.character.as_ref() {
                Some(c) => {
                    let p = c.position();
                    (p.cell.0, self.render_frame_of(ws, p))
                }
                None => (0, Frame::new(ws.camera.position, Quat::IDENTITY)),
            }
        }

        /// The frame's light pools and the env-cell burn-in.
        ///
        /// * On a cell change, clear the static-light count and rebuild the static pool from every
        ///   visible cell's light records whose `state & 1` is set. The current degrade level sets
        ///   the static and dynamic pool caps.
        /// * On every frame, clear the dynamic-light count, add the viewer light, then add every
        ///   non-static light record of the visible environment cells. A light belongs to a cell
        ///   only after light registration, so an object standing in a land cell, or held after
        ///   `leave_world`, contributes nothing.
        /// * Environment-cell drawing updates static vertex colors for every cell whose burn is
        ///   not at the current static-light count.
        fn update_world_lights(&mut self, ws: &mut WorldState) {
            if !self.cfg.object_lighting {
                return;
            }
            let (cell, player) = self.player_origin(ws);
            let keys: Vec<(i32, i32)> = self.blocks.keys().copied().collect();
            let key = (cell, keys);
            if self.static_pool_key.as_ref() != Some(&key) {
                let level = self.degrade.governor.level();
                self.light_pools.max_static = level.max_static_lights;
                self.light_pools.max_dynamic = level.max_dynamic_lights;
                self.light_pools.clear_statics();
                for block in self.blocks.values() {
                    for l in block.cell_lights.iter().filter(|l| l.is_static) {
                        // The offset from the player block to the light cell, plus `frame.origin`,
                        // gives the object's frame in the player's space, which
                        // in this build is render space -- the block origin folded in.
                        let frame = Frame::new(
                            Vec3::new(
                                l.frame.origin.x + block.origin.0,
                                l.frame.origin.y + block.origin.1,
                                l.frame.origin.z,
                            ),
                            l.frame.rotation,
                        );
                        self.light_pools
                            .add_static_from(&l.info, l.cell, &frame, player.origin);
                    }
                }
                self.static_pool_key = Some(key);
            }
            self.light_pools.clear_dynamics();
            let has_player = ws.character.is_some();
            self.light_pools.add_dynamic_from(
                &viewer_light(has_player),
                CellId(cell),
                &player,
                player.origin,
            );
            for o in ws.objects.values() {
                if !o.drawn {
                    continue;
                }
                let Some(pos) = o.sim.position else { continue };
                if dereth_physics::landdefs::is_outdoors(pos.cell) {
                    continue;
                }
                if !o.sim.state.is_lighting_on() || o.sim.state.is_static() {
                    continue;
                }
                let driver = o.sim.driver.borrow();
                let Some(setup) = driver.part_array.setup.as_ref() else {
                    continue;
                };
                for info in setup_data_lights(setup) {
                    self.light_pools
                        .add_dynamic_from(&info, pos.cell, &o.frame, player.origin);
                }
            }
            if let Some(c) = ws.character.as_ref() {
                let pos = c.position();
                if !dereth_physics::landdefs::is_outdoors(pos.cell)
                    && ws.character_state.is_lighting_on()
                {
                    let frame = self.render_frame_of(ws, pos);
                    let driver = c.driver();
                    if let Some(setup) = driver.part_array.setup.as_ref() {
                        for info in setup_data_lights(setup) {
                            self.light_pools.add_dynamic_from(
                                &info,
                                pos.cell,
                                &frame,
                                player.origin,
                            );
                        }
                    }
                }
            }
            let count = self.light_pools.statics.len();
            let pools = &self.light_pools;
            for block in self.blocks.values_mut() {
                let origin = block.origin;
                for cell in &mut block.env_cells {
                    if cell.burned_count == Some(count) {
                        continue;
                    }
                    burn_env_cell(cell, pools, origin);
                    cell.burned_count = Some(count);
                }
            }
        }

        /// The scene's sunlight description, as the tail of
        /// the light update builds it from the landscape's sun direction and
        /// `sunlight_color` (which the normal-mode render and the position change copy into
        /// the world light list for an outdoor or `seen_outside` viewer). `None` while the sun vector is
        /// below the `0.0002` gate, which is "a light that contributes nothing".
        #[must_use]
        pub fn sun_light(&self) -> Option<D3dLight> {
            let l = &self.land.lighting;
            let c = [
                f32::from(l.sunlight_color[0]) * BYTE_TO_FLOAT,
                f32::from(l.sunlight_color[1]) * BYTE_TO_FLOAT,
                f32::from(l.sunlight_color[2]) * BYTE_TO_FLOAT,
            ];
            sunlight_light(l.sunlight, c)
        }

        /// World-light ambient color follows the two position-change arms: an
        /// outdoor or `seen_outside` viewer gets
        /// the landscape's calculated object-light level and ambient color;
        /// any other interior gets world ambient light 0.2 in colour `0xFFFFFFFF`.
        /// Landscape-lighting setup re-issues the
        /// first on every light tick, which cannot run for a viewer the landscape released.
        #[must_use]
        pub fn world_ambient_color(&self, ws: &WorldState) -> [f32; 3] {
            let indoors_unseen = ws.viewer_cell().is_some_and(|c| !ws.cell_seen_outside(c));
            if indoors_unseen {
                return world_ambient(INDOOR_AMBIENT_LEVEL, 0xFFFF_FFFF);
            }
            let l = &self.land.lighting;
            let c = (u32::from(l.ambient_color[0]) << 16)
                | (u32::from(l.ambient_color[1]) << 8)
                | u32::from(l.ambient_color[2]);
            world_ambient(calc_object_light(l.sunlight, l.ambient_level), c)
        }

        /// The `D3DLIGHT9`s one object mesh is drawn with:
        /// sunlight use set to 1 (the sun alone) for the landscape's outdoor objects, and
        /// every dynamic light in distance order
        /// whose falloff sphere reaches the object's drawing sphere, then the statics, eight at
        /// most -- for everything the inner mesh draw draws with sunlight use 0.
        /// `centre`/`radius` are the drawing sphere in render space. Empty with the lighting
        /// switch off.
        #[must_use]
        pub fn object_light_set(&self, centre: Vec3, radius: f32, outdoors: bool) -> Vec<D3dLight> {
            if !self.cfg.object_lighting {
                return Vec::new();
            }
            let sun = self.sun_light();
            let active = if outdoors {
                let mut a = ActiveLights::default();
                use_sunlight_set(&mut a, true);
                a
            } else {
                minimize_object_lighting(&self.light_pools, centre, radius)
            };
            enabled_lights(&active, &self.light_pools, sun.as_ref())
        }

        /// The light set for a cell mesh:
        /// every dynamic light; the statics are in the vertices.
        #[must_use]
        pub fn envcell_light_set(&self) -> Vec<D3dLight> {
            if !self.cfg.object_lighting {
                return Vec::new();
            }
            let active = minimize_envcell_lighting(&self.light_pools);
            enabled_lights(&active, &self.light_pools, None)
        }

        /// The light pools as they stand, for the tests.
        #[must_use]
        pub fn light_pools(&self) -> &LightPools {
            &self.light_pools
        }

        /// How many light objects the resident blocks' interior cells registered.
        #[must_use]
        pub fn cell_light_objects(&self) -> usize {
            self.blocks.values().map(|b| b.cell_lights.len()).sum()
        }

        /// One interior cell's vertices as they will be uploaded: block-local
        /// position, block-local normal, and the burned `D3DFVF_DIFFUSE` RGB. A probe for the
        /// tests; empty for a cell no resident block holds.
        #[must_use]
        pub fn env_cell_burned_vertices(&self, cell: CellId) -> Vec<(Vec3, Vec3, [u8; 3])> {
            let mut out = Vec::new();
            for block in self.blocks.values() {
                for c in block.env_cells.iter().filter(|c| c.id == cell) {
                    for m in &c.meshes {
                        for v in m.vertices.as_chunks::<OBJECT_VERTEX_STRIDE>().0 {
                            let f =
                                |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
                            let o = OBJECT_DIFFUSE_OFFSET;
                            out.push((
                                Vec3::new(f(0), f(4), f(8)),
                                Vec3::new(f(12), f(16), f(20)),
                                [v[o + 2], v[o + 1], v[o]],
                            ));
                        }
                    }
                }
            }
            out
        }

        /// One interior cell's static batches as `draw_cell_statics` lights them:
        /// sphere center in render space, radius, lights bound, vertex counts, luminosity,
        /// alpha-list membership, and whether the draw is chunked. A probe for the tests.
        #[must_use]
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        pub fn env_cell_static_light_probe(
            &self,
            cell: CellId,
        ) -> Vec<(Vec3, f32, usize, usize, usize, f32, bool, bool)> {
            let mut out = Vec::new();
            for block in self.blocks.values() {
                let origin = block.origin;
                for c in block.env_cells.iter().filter(|c| c.id == cell) {
                    for b in c.statics.iter().chain(c.statics_blended.iter()) {
                        let centre = Vec3::new(
                            b.sphere.0.x + origin.0,
                            b.sphere.0.y + origin.1,
                            b.sphere.0.z,
                        );
                        let lights = if self.cfg.object_lighting {
                            self.object_light_set(centre, b.sphere.1, false).len()
                        } else {
                            0
                        };
                        let verts = drawn_vertices(b);
                        let n = verts.len() / OBJECT_VERTEX_STRIDE;
                        let zero = verts
                            .as_chunks::<OBJECT_VERTEX_STRIDE>()
                            .0
                            .iter()
                            .filter(|v| v[12..24].iter().all(|&x| x == 0))
                            .count();
                        out.push((
                            centre,
                            b.sphere.1,
                            lights,
                            n,
                            zero,
                            b.luminosity,
                            b.key.alpha_blend,
                            !b.chunks.is_empty(),
                        ));
                    }
                }
            }
            out
        }

        /// The block origin a cell's block-local frames are measured from, for
        /// the tests; `None` for a cell no resident block holds.
        #[must_use]
        pub fn cell_block_origin(&self, cell: CellId) -> Option<(f32, f32)> {
            self.blocks
                .values()
                .find(|b| b.env_cells.iter().any(|c| c.id == cell))
                .map(|b| b.origin)
        }

        /// The outdoor pass: draw sky pass 0, every resident
        /// block's terrain, buildings and statics, the alpha list, then draw sky pass 1.
        ///
        /// **This is its own function because it has two callers, and retail's has
        /// two callers.** Normal world rendering reaches it directly only on
        /// the **outdoors** branch (`(viewer.objcell_id & 0xFFFF) < 0x100`); the indoor branch
        /// draws the interior, whose cell-rendering first step is: if the outside view has any
        /// views, install it as the active portal list and draw the landscape.
        /// So an indoor viewer gets the outdoor world **only** through a portal chain that
        /// reached one, and a dungeon with no outdoor portal in view gets none of it — no
        /// terrain, no scenery and **no sky**. See [`Self::draw_inside`] and
        /// [`SceneConfig::outside_view_gate`].
        ///
        /// # Errors
        /// Any failure from the runtime.
        ///
        /// # Returns
        /// The exact building-interior cells this landscape pass reached.
        fn draw_lscape(
            &self,
            ws: &WorldState,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
            sky_per_frame: &PerFrameConstants,
            outside: bool,
        ) -> Result<BTreeSet<u32>, RenderError> {
            // The landscape pass runs under the sunlight-only light set
            // in normal rendering: the sun is the
            // only light every outdoor batch is drawn with. It is set above the
            // pass-0 sky, because sky drawing is inside that same bracket — the sunlight switch
            // is issued before the outdoor draw, whose first call draws sky pass 0 —
            // and the sky's meshes are lit by it like everything else
            // (the subset draw turns fixed-function lighting on unconditionally).
            let sun_set = self
                .cfg
                .object_lighting
                .then(|| self.object_light_set(Vec3::ZERO, 0.0, true));
            let sky_lights: &[D3dLight] = sun_set.as_deref().unwrap_or(&[]);

            // --- the sky, pass 0 -----------------------------------------------------------
            // opens with sky pass 0, before any block.
            if let Some(sky) = &self.sky {
                sky.draw(
                    gpu,
                    dereth_world_render::sky::SkyPass::Before,
                    sky_per_frame,
                    ws.camera.position,
                    outside,
                    sky_lights,
                )?;
            }

            // --- the outdoor portal machinery's per-frame input ----------------------------
            // Built once for the whole frame rather than per building, because a
            // building's portals reach cells of any resident block. Building drawing is reached
            // through even when that landscape is seen from an interior's outside
            // portal (cell drawing). In the villa courtyard the camera can occupy the
            // front-gate env cell while looking across outdoors into the villa's other doorway;
            // rejecting the building pass merely because that camera is indoors loses the room
            // and its door. The caller already omits landscape drawing entirely for a sealed indoor view.
            let building_pass = self.cfg.building_portals
                && self.blocks.values().any(|b| !b.building_views.is_empty())
                && self.blocks.values().any(|b| !b.env_cells.is_empty());
            let interiors = building_pass.then(|| self.traversal_cells(ws));
            // Cell rendering opens by clearing its drawn-cell set, so a cell reached through two
            // buildings' openings is drawn once.
            let mut drawn_cells: std::collections::HashSet<u32> = std::collections::HashSet::new();
            // Set building translucency to 0.0, the first thing building drawing does.
            // See [`Self::flush_alpha_list`].
            let mut alpha_flushed: std::collections::HashSet<(i32, i32)> =
                std::collections::HashSet::new();
            let mut alpha_pending: Vec<(i32, i32)> = Vec::new();

            // --- the landscape -------------------------------------------------------------
            let mid = ws.streamer.window.mid_width();
            let mut vertices: Vec<u8> = Vec::with_capacity(6 * LAND_VERTEX_STRIDE as usize);
            // The landscape detail surface, when `Render.LandscapeDetailTextures` made one, and the
            // camera frame its distance fade is measured in: a point's view-space depth is its
            // distance along the camera's forward (local y) axis.
            let land_detail =
                self.current_detail(dereth_world_render::detail::DetailClass::Landscape);
            let camera = ws.camera.frame();
            let mut detail_vertices: Vec<u8> = Vec::with_capacity(6 * LAND_VERTEX_STRIDE as usize);
            let order: Vec<(u32, u32)> = block_draw_order(mid)
                .into_iter()
                .map(|i| (i / mid, i % mid))
                .collect();
            for &(xi, yi) in &order {
                let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                    continue;
                };
                let Some(block) = self.blocks.get(&(slot.block_x, slot.block_y)) else {
                    continue;
                };
                let Some(mesh) = slot.mesh.as_ref().and_then(SlotMesh::get::<LandblockMesh>) else {
                    continue;
                };
                // Vertices are block-local, so the viewpoint the back-face test uses has to be too.
                let viewpoint = Vec3::new(
                    ws.camera.position.x - block.origin.0,
                    ws.camera.position.y - block.origin.1,
                    ws.camera.position.z,
                );
                let world = world_constants(&Frame::new(
                    Vec3::new(block.origin.0, block.origin.1, 0.0),
                    Quat::IDENTITY,
                ));
                let n = mesh.side_cell_count;
                // A block outside the viewer's own is stitched towards it, and `closest_cell`
                // reads `trans_dir` to decide which of its cells is nearest.
                let step = 8 / u16::from(n).max(1);
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: both are clamped to side_cell_count - 1 <= 7 on the line above. Not a
                // float conversion.
                let viewer_cell = (
                    (u16::from(ws.streamer.viewer_cell.0) / step).min(u16::from(n) - 1) as u8,
                    (u16::from(ws.streamer.viewer_cell.1) / step).min(u16::from(n) - 1) as u8,
                );
                for cell in cell_draw_order(n, mesh.trans_dir, viewer_cell) {
                    let (i, j) = (
                        usize::from(cell) / usize::from(n),
                        usize::from(cell) % usize::from(n),
                    );
                    let tris = visible_triangles(mesh, i, j, viewpoint);
                    if !tris.is_empty() {
                        vertices.clear();
                        for p in &tris {
                            for v in triangle_vertices(mesh, p) {
                                vertices.extend_from_slice(&v.to_bytes());
                            }
                        }
                        // The current mode's representation, or the other one while a switch is
                        // still converting this block.
                        let has_composites = !block.cell_keys.is_empty();
                        let splat = if self.terrain_splat || !has_composites {
                            block
                                .cell_splat
                                .get(usize::from(cell))
                                .and_then(Option::as_ref)
                        } else {
                            None
                        };
                        if let Some(splat) = splat {
                            gpu.draw_terrain_splat(
                                &self.terrain_key,
                                splat,
                                per_frame,
                                &world,
                                &vertices,
                            )?;
                        } else {
                            if let Some(slot) =
                                block.cell_texture.get(usize::from(cell)).copied().flatten()
                            {
                                // Sampler 1 = linear / clamp: a merged land surface covers exactly
                                // one cell and the UVs are exactly [0, 1], so wrapping would fetch
                                // across the seam.
                                gpu.bind_texture(slot, 1);
                            }
                            gpu.draw_dynamic(
                                &self.terrain_key,
                                &DrawConstants::default(),
                                per_frame,
                                &world,
                                &vertices,
                            )?;
                        }
                        // The landscape detail pass: a block at full detail draws each cell's
                        // triangles again with the detail texture right after the ground.
                        if let (Some((slot, tiling)), true) = (
                            land_detail,
                            dereth_world_render::detail::block_takes_landscape_detail(u32::from(n)),
                        ) {
                            detail_vertices.clear();
                            for p in &tris {
                                for v in triangle_vertices(mesh, p) {
                                    let eye = dereth_physics::math::globaltolocal(
                                        &camera,
                                        Vec3::new(
                                            block.origin.0 + v.pos[0],
                                            block.origin.1 + v.pos[1],
                                            v.pos[2],
                                        ),
                                    );
                                    detail_vertices.extend_from_slice(
                                        &detail_vertex(v, eye.y, tiling).to_bytes(),
                                    );
                                }
                            }
                            // Sampler 0 = linear / wrap: the detail texture repeats across the
                            // cell.
                            gpu.bind_texture(slot, 0);
                            gpu.draw_dynamic(
                                &self.terrain_detail_key,
                                &DrawConstants::default(),
                                per_frame,
                                &world,
                                &detail_vertices,
                            )?;
                        }
                    }

                    // Block drawing reaches a building through its owning land cell's
                    // sort-cell draw: terrain first, then building drawing, while the remaining cells
                    // continue far to near. Delaying every building until after all 64 terrain
                    // cells lets a far opening's ALWAYS depth stamp erase nearer terrain.
                    let draw_cell = cell;
                    if let Some((cells, placed)) = interiors.as_ref() {
                        let has_building = block
                            .building_views
                            .iter()
                            .any(|b| b.draw_cell(n) == draw_cell);
                        if has_building && self.cfg.portal_alpha_flush {
                            self.flush_alpha_list(
                                gpu,
                                per_frame,
                                &alpha_pending,
                                &mut alpha_flushed,
                                AlphaFlush::Building,
                            )?;
                            alpha_pending.clear();
                        }
                        self.draw_building_interiors(
                            ws,
                            gpu,
                            per_frame,
                            block,
                            draw_cell,
                            n,
                            cells,
                            placed,
                            &mut drawn_cells,
                        )?;
                    }
                }

                // --- this block's scenery, buildings and static objects ---------------------
                // Block drawing draws each land cell's terrain and then
                // its sort cell's building and objects, all inside the block's own
                // level-3 position push of (block id, identity). The batches are block-local, so
                // `world` above is exactly that push.
                for batch in &block.opaque {
                    submit_static_batch(
                        gpu,
                        per_frame,
                        &world,
                        batch,
                        sun_set.as_deref(),
                        self.current_detail(dereth_world_render::detail::DetailClass::Building),
                    )?;
                }
                // "Multiple Pass Alpha": a clip-mapped subset is drawn in place, where its cell
                // draws it, **and** queued on the clip list, so its second pass goes out with the
                // next flush (the next building's, or the frame's) ahead of everything on the
                // alpha list. Drawing the first pass here, inside the walk, is what lets a
                // building's own flush carry the second.
                if self.cfg.render.multi_pass_alpha {
                    let detail =
                        self.current_detail(dereth_world_render::detail::DetailClass::Building);
                    let mut queued = false;
                    let mut stats = self.frame_landscape_alpha.get();
                    for batch in block
                        .blended
                        .iter()
                        .filter(|b| static_multipass_member(b, detail.is_some()))
                    {
                        submit_static_batch(
                            gpu,
                            per_frame,
                            &world,
                            batch,
                            sun_set.as_deref(),
                            detail,
                        )?;
                        stats.clip += usize::from(batch.key.alpha_test);
                        queued = true;
                    }
                    self.frame_landscape_alpha.set(stats);
                    if queued {
                        self.frame_multipass_pending
                            .borrow_mut()
                            .push((slot.block_x, slot.block_y));
                    }
                }
                // This block's translucency is now "queued": the next building's
                // `FlushAlphaList(0.0)` is what draws it, and the frame's own is what draws the
                // rest.
                alpha_pending.push((slot.block_x, slot.block_y));
            }

            // --- the alpha-tested list -----------------------------------------------------
            // `BlockDraw::blended` is keyed on `alpha_blend` alone, so
            // it holds **both** lists: the alpha-tested cut-outs (catalogue rows 7-9, depth write
            // **kept**) and the blended ones (rows 3-6 and 10-13, depth write off). Drawing the
            // bucket as one loop in bake order would interleave them.
            //
            // The two halves are separated here. The alpha-tested half stays
            // in the landscape pass, drawn in place — a standing deviation, and a harmless one:
            // an alpha-tested surface writes depth, so where it sits
            // among the other depth-writing draws cannot change which of them is in front. The
            // blended half does not write depth and is ordered by nothing but when it is drawn, so
            // it goes on the queue below and is drawn at the frame's flush.
            //
            // Drawing the alpha-tested half first is the client's own flush order — that list to
            // exhaustion, then the blended one — and it explains the observed case of
            // distant tree foliage painting over a near plant: the trees are alpha-tested and the
            // plant is not.
            for &(xi, yi) in &order {
                let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                    continue;
                };
                let Some(block) = self.blocks.get(&(slot.block_x, slot.block_y)) else {
                    continue;
                };
                // With "Multiple Pass Alpha" on, the clip-mapped batches were drawn inside the
                // walk; what is left here is the alpha-tested rest (an `Alpha | ClipMap` surface,
                // a shell under the building detail texture).
                let detail =
                    self.current_detail(dereth_world_render::detail::DetailClass::Building);
                let in_walk = |b: &StaticBatch| {
                    self.cfg.render.multi_pass_alpha && static_multipass_member(b, detail.is_some())
                };
                let here = |b: &&StaticBatch| static_clip_list_member(b) && !in_walk(b);
                if !block.blended.iter().any(|b| here(&b)) {
                    continue;
                }
                let world = world_constants(&Frame::new(
                    Vec3::new(block.origin.0, block.origin.1, 0.0),
                    Quat::IDENTITY,
                ));
                let mut stats = self.frame_landscape_alpha.get();
                for batch in block.blended.iter().filter(here) {
                    submit_static_batch(
                        gpu,
                        per_frame,
                        &world,
                        batch,
                        sun_set.as_deref(),
                        self.current_detail(dereth_world_render::detail::DetailClass::Building),
                    )?;
                    stats.clip += 1;
                    // How many of the frame flush's blended batches were already on the screen
                    // when this alpha-tested one went down. See [`LandscapeAlphaStats`].
                    stats.blend_before_clip = stats.blend_before_clip.max(stats.blend);
                }
                self.frame_landscape_alpha.set(stats);
            }

            // --- the blended list, queued --------------------------------------------------
            // This pass does **not** flush. The landscape walk has no
            // flush in it at all: the flushes are the per-building one above and then the
            // caller's, once the objects are down. In the client those objects are the land
            // cells' own, drawn inside the block walk, so every opaque creature standing in an
            // outdoor cell is already on the screen with its depth written before a single
            // blended leaf is.
            //
            // Drawing the queue here instead put the landscape's translucency **under** the
            // objects, and a blended surface writes no depth (catalogue rows 3-6 and 10-13), so an
            // object standing behind a plant passed `ZFunc::Less` where its leaves were and
            // painted over them. See [`Self::flush_pending_alpha_list`].
            self.frame_alpha_pending
                .borrow_mut()
                .extend(alpha_pending.iter().copied());

            // --- the sky, pass 1 -----------------------------------------------------------
            // closes with sky pass 1, after the blocks and their alpha list
            // and before `WorldObjects` draws the objects — which is why a nearby NPC still covers the
            // weather layer even though the layer was painted with `DEPTHTEST_ALWAYS`.
            if let Some(sky) = &self.sky {
                sky.draw(
                    gpu,
                    dereth_world_render::sky::SkyPass::After,
                    sky_per_frame,
                    ws.camera.position,
                    outside,
                    sky_lights,
                )?;
            }

            // Building drawing's own cell walk — the interiors an outdoor viewer
            // sees through a building's openings. `drawn_cells` is that walk's
            // set, so it is exactly the cells this pass put on the screen. See
            // [`Self::frame_drawn_cells`].
            if let Some(seen) = self.frame_drawn_cells.borrow_mut().as_mut() {
                seen.extend(drawn_cells.iter().copied());
            }
            Ok(drawn_cells.into_iter().collect())
        }

        /// Draw one frame of the world in normal render mode.
        ///
        /// The outdoor pass itself is `Self::draw_lscape`, because retail's normal-mode render
        /// reaches it on its **outdoors** branch alone and the indoor
        /// branch reaches it only through [`Self::draw_inside`].
        ///
        /// Blocks are walked **outermost ring first** and cells within a block **farthest first**,
        /// which is `block_draw_order` and `cell_draw_order` verbatim; the order is
        /// preserved here even though the terrain is opaque
        /// and z-buffered.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn draw(&self, ws: &WorldState, gpu: &mut Gpu) -> Result<(), RenderError> {
            // **The world is drawn inside the world-view element's rectangle.**
            //
            // In the client the device viewport is *state*, set once per layout change by the
            // default viewport calculation and then by the viewport-size notice handler. Ending
            // the frame saves it, sets the full display for the 2D
            // overlay pass, and puts it back.
            // `Gpu::begin_frame` here re-arms the full back buffer every frame, so the equivalent
            // is to bracket the scene pass: the world gets the rect, and everything after this
            // call -- `Renderer::draw_ui`, which is the frame presentation's overlay -- gets the window again.
            //
            // Reset on the error path too. A `?` that left a sub-rect installed would silently
            // clip the UI of every frame after the first failure.
            let (w, h) = gpu.size();
            gpu.set_viewport(self.effective_viewport(ws, w, h));
            let r = self.draw_in_viewport(ws, gpu);
            gpu.reset_viewport();
            r
        }

        /// [`Self::draw`]'s body, with the device viewport already installed.
        fn draw_in_viewport(&self, ws: &WorldState, gpu: &mut Gpu) -> Result<(), RenderError> {
            // The frame's cell walk starts here, so the previous frame's answer is
            // replaced rather than accumulated onto. See [`Self::frame_drawn_cells`].
            *self.frame_drawn_cells.borrow_mut() = Some(std::collections::BTreeSet::new());
            // The frame's offered-object set starts here for the same reason, and
            // is accumulated by both of a split frame's object passes. See
            // [`Self::frame_pick_candidates`].
            *self.frame_pick_candidates.borrow_mut() = Some(std::collections::BTreeSet::new());
            // Same bracket: an outdoor frame, or an indoor one facing a wall,
            // must not report the previous frame's openings.
            self.frame_outside_views.borrow_mut().clear();
            self.frame_outside_view_count.set(None);
            self.frame_cell_views.borrow_mut().clear();
            self.frame_portal_stamps.set((0, 0));
            // Same bracket: the landscape's alpha queue and its census.
            // The queue is cleared as well as the census, so a frame that failed part way through
            // cannot leak its unflushed blocks into the next one's flush.
            self.frame_alpha_pending.borrow_mut().clear();
            self.frame_multipass_pending.borrow_mut().clear();
            self.frame_alpha_order.borrow_mut().clear();
            self.frame_blend_order.borrow_mut().clear();
            self.frame_landscape_alpha
                .set(LandscapeAlphaStats::default());
            let (w, h) = gpu.size();
            let view = self.view_params(ws, w, h);
            let per_frame = per_frame_constants(&view);
            // Sky drawing rebuilds the projection with `zfar * 4` for the duration of both
            // passes and puts it back afterwards, so the sky gets its own `PerFrame` block and the
            // world keeps the one above.
            let sky_view = crate::sky::SkyScene::view_params(&view);
            let sky_per_frame = per_frame_constants(&sky_view);
            let outside = ws.viewer_cell().is_none();

            // --- the outdoor pass, or not --------------------------------------------------
            // Normal world rendering branches on the **viewer's** own cell:
            // outdoors it draws the landscape; indoors it draws the interior and
            // nothing else. The outdoor world reaches an indoor frame only through
            // cell rendering's `outside_view`, which [`Self::draw_inside`] issues
            // as its first step.
            if outside || !self.cfg.outside_view_gate {
                let _ = self.draw_lscape(ws, gpu, &per_frame, &sky_per_frame, outside)?;
            }

            // --- the objects, in two phases ------------------------------------------------
            // The split follows the depth clear rather than object ownership.
            //
            // Drawing the world through an opening first installs `outside_view`,
            // draws the outdoor blocks and their land-cell objects, and flushes the alpha list at
            // 0.0. It then clears depth, stamps the openings back, draws the interior cells, and
            // finally draws the interior objects.
            //
            // So an object standing in an **outdoor** cell is on the screen *before* the depth
            // clear and the stamps, and an object in an interior cell after them. Drawing
            // every object -- indoor, outdoor and the local body -- from here, after
            // [`Self::draw_inside`] had returned, would make anything past the doorway fail
            // `ZFunc::Less` against the depth the portal-polygon draw's mask 6 had just written
            // at the opening's own `z/w`, and not be drawn at all: a door swinging outside
            // would disappear as it opened, and the character would disappear until the camera
            // moved all the way outside.
            //
            // **Normal rendering's two arms are exclusive**, which is why the outdoor half has to
            // be issued from inside `draw_inside` and not from the
            // `if outside` above: an indoor viewer takes the interior branch, and its only route
            // to the outdoor pass is the cell renderer's outside-view step.
            //
            // The split is made **only on the frames retail splits**: `draw_inside` runs
            // `after_outdoors` exactly when it reached `IndoorStep::OutdoorsThroughPortals`, i.e.
            // when the viewer is indoors and an opening is in view. Every other frame -- an
            // outdoor viewer, and a dungeon with no outdoor portal in sight -- takes
            // [`ObjectPhase::All`] and is the single sorted pass this build has always had.
            let material = self.cfg.material_translucency;
            let mut cone = ObjectConeStats::default();
            let mut alpha = AlphaListStats::default();
            let mut counts = (0u32, 0u32);
            let mut trace: Vec<PartSubsetDraw> = Vec::new();
            let mut split = false;
            let mut interior_cells = BTreeSet::new();
            let mut particle_stats = crate::particles::ParticleStats::default();

            // --- the indoor path ------------------------------------------------------------
            {
                let (c, a, n, t, s, ps) = (
                    &mut cone,
                    &mut alpha,
                    &mut counts,
                    &mut trace,
                    &mut split,
                    &mut particle_stats,
                );
                let mut after_outdoors =
                    |gpu: &mut Gpu,
                     building_cells: &BTreeSet<u32>,
                     outside_views: &[dereth_world_render::cells::clip::ViewPoly]|
                     -> Result<(), RenderError> {
                        *s = true;
                        let particles = if self.cfg.particles {
                            self.collect_particles(ws, Some(building_cells), true)
                        } else {
                            Vec::new()
                        };
                        self.draw_object_pass(
                            ws,
                            gpu,
                            &view,
                            &per_frame,
                            material,
                            ObjectPhase::Outdoors,
                            Some(building_cells),
                            (!outside_views.is_empty()).then_some(outside_views),
                            // This pass runs under `&outside_view`, not under any cell's
                            // own `portal_view`, so it takes no per-cell map.
                            None,
                            c,
                            a,
                            n,
                            t,
                            &mut |gpu| self.drain_multipass_pending(gpu, &per_frame),
                            &particles,
                            ps,
                        )?;
                        Ok(())
                    };
                self.draw_inside(
                    ws,
                    gpu,
                    &per_frame,
                    &sky_per_frame,
                    &mut interior_cells,
                    &mut after_outdoors,
                )?;
            }
            // Particle parts are ordinary shadow parts of their one owning cell. On a
            // split indoor frame, outdoor/building-cell particles were already drawn with the
            // pre-clear object pass; this stage takes only particles in cells reached by the
            // main portal traversal after the clear. An unsplit frame retains the original one
            // global set. The object pass sorts them with the parts and draws them through the
            // same two lists.
            let parts = if self.cfg.particles {
                if split {
                    self.collect_particles(ws, Some(&interior_cells), false)
                } else {
                    self.collect_particles(ws, None, true)
                }
            } else {
                Vec::new()
            };
            self.draw_object_pass(
                ws,
                gpu,
                &view,
                &per_frame,
                material,
                if split {
                    ObjectPhase::Interior
                } else {
                    ObjectPhase::All
                },
                split.then_some(&interior_cells),
                // The post-clear pass runs under each cell's **own** `portal_view`, not
                // `outside_view`. The draw publishes them.
                None,
                Some(&*self.frame_cell_views.borrow()),
                &mut cone,
                &mut alpha,
                &mut counts,
                &mut trace,
                &mut |gpu| self.drain_multipass_pending(gpu, &per_frame),
                &parts,
                &mut particle_stats,
            )?;
            self.frame_object_cone.set(cone);
            self.frame_alpha_lists.set(alpha);
            self.frame_material_parts.set(counts);
            *self.frame_part_order.borrow_mut() = trace;

            // --- the alpha flush -----------------------------------------------------------
            // The world pass's own line, the one after its two
            // exclusive arms: the landscape walk draws the blocks, their buildings **and their
            // objects**, and the alpha flush comes after all three. So the landscape's
            // translucency is the **last** world geometry on the screen, drawn over every opaque
            // object the walk put there. A split indoor frame has already drained the queue at
            // [`IndoorStep::FlushBeforeClear`], and this call finds it empty.
            self.flush_pending_alpha_list(gpu, &per_frame)?;

            self.frame_particles.set(particle_stats);
            Ok(())
        }

        /// One of cell rendering's two object passes.
        ///
        /// The object half of [`Self::draw_in_viewport`], parameterised by
        /// which cells' objects this call is for. See the call site for the retail order the split
        /// comes from; [`ObjectPhase::All`] is the un-split pass and is what every frame that does
        /// not reach `IndoorStep::OutdoorsThroughPortals` still takes.
        ///
        /// The four `_out` parameters are **accumulated into**, not written, because a split frame
        /// makes this call twice and the frame's published counters are the union of the two.
        ///
        /// # Errors
        /// Any failure from the runtime.
        #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
        fn draw_object_pass(
            &self,
            ws: &WorldState,
            gpu: &mut Gpu,
            view: &dereth_render::camera::ViewParams,
            per_frame: &PerFrameConstants,
            material: bool,
            phase: ObjectPhase,
            visible_cells: Option<&BTreeSet<u32>>,
            // When this pass is the one cell drawing issues under its outside-view
            // list, `Some` puts every
            // submission down the mesh renderer's **portal** arm — one
            // view-cone check per view polygon, combined across the view list —
            // instead of the no-portal-list arm's single full-screen cone. `None` is every
            // other pass and the unclipped traversal.
            //
            // This pass also carries the objects of the **building** cells building drawing's own
            // traversal reached, and retail re-installs each of those cells' narrower
            // `portal_view` before drawing them through cell drawing's mode 1.
            // Testing them against `outside_view` — the parent those views were clipped out of —
            // is therefore a **superset** of retail's answer, in the same one-directional sense
            // as the unclipped traversal: nothing retail drew is dropped.
            outside_views: Option<&[dereth_world_render::cells::clip::ViewPoly]>,
            // The newest portal view per interior cell, which
            // cell rendering installs as the portal list before that cell's
            // per-cell object draw. A submission whose cell is in the map is coned
            // against *its* polygons; everything else keeps the full-screen cone.
            cell_views: Option<&BTreeMap<u32, Vec<dereth_world_render::cells::clip::ViewPoly>>>,
            cone_out: &mut ObjectConeStats,
            alpha_out: &mut AlphaListStats,
            counts_out: &mut (u32, u32),
            trace_out: &mut Vec<PartSubsetDraw>,
            // The rest of this flush's **clip list**, drawn after the objects' clip entries and
            // before their blend entries: the second passes "Multiple Pass Alpha" queued for the
            // landscape's statics. The client has one pair of lists and drains the whole clip
            // list before any of the alpha list, so a translucent object blends over the soft
            // edge of a cut-out instead of under it.
            clip_tail: &mut dyn FnMut(&mut Gpu) -> Result<(), RenderError>,
            // This stage's particles. An emitter's particles are parts of their object: they are
            // sorted by viewer distance with every other part and go through the same per-subset
            // classification and the same two lists, so a far emitter's smoke is drawn before a
            // nearer translucent object and not over it.
            particles: &[crate::particles::ParticlePart],
            particle_stats: &mut crate::particles::ParticleStats,
        ) -> Result<(), RenderError> {
            // The landscape half takes the objects in each outdoor landcell and in the building
            // env cells that building drawing reached. The per-cell object draw's later half takes the
            // objects registered in the main interior cells the portal traversal actually reached.
            //
            // A held object's wire position is absent in this model, but parent
            // assignment immediately moves the child into its parent's cell after leaving its old
            // world placement, and cell entry recurses through
            // children. So the same cell walk draws the holder and everything it carries.
            // [`Self::object_draw_cell`] recovers that effective cell for direct/nested remote
            // holders and for the local body, which has no [`SceneObject`] of its own.
            // Native does not have a global "all interior objects" pass.
            // Cell rendering calls the per-cell object draw only for
            // `cell_draw_list`. The outdoor building path reaches that same call through
            // view construction, then cell drawing's mode 1. A fully
            // interior object is in those lists through the interior-cell insertion path;
            // an object that crosses a boundary has one shadow registration per
            // overlapped cell. Use that same overlap set when it is available, so culling by the
            // object's origin does not make a doorway-spanning door disappear.
            let drawn_cells = self.frame_drawn_cells.borrow();
            let seen = visible_cells.or(drawn_cells.as_ref());
            let viewer_inside = ws.viewer_cell().is_some();
            let interior_seen =
                |cell: Option<CellId>, handle: Option<dereth_physics::PhysHandle>| {
                    let Some(cell) = cell else { return false };
                    // A doorway-spanning part can retain its outdoor origin and also be registered in
                    // a reached env cell. Native cell drawing walks that env cell's shadow-part list
                    // after its outdoor draw, frame-stamp increment and depth clear, so the same part
                    // is eligible on both sides of the reset. Prefer the physical registration when
                    // it exists; the origin-cell fallback below is only for a body without shadows.
                    if let Some(body) =
                        handle.and_then(|h| ws.character.as_ref().and_then(|c| c.world.get(h)))
                    {
                        if !body.shadow_objects.is_empty() {
                            return body.shadow_objects.iter().any(|shadow| {
                                shadow.cell_present
                                    && !dereth_physics::landdefs::is_outdoors(shadow.cell_id)
                                    && seen.is_some_and(|cells| cells.contains(&shadow.cell_id.0))
                            });
                        }
                    }
                    if dereth_physics::landdefs::is_outdoors(cell) {
                        return false;
                    }
                    seen.is_some_and(|cells| cells.contains(&cell.0))
                };
            let wanted_light =
                |cell: Option<CellId>, handle: Option<dereth_physics::PhysHandle>| {
                    let outdoors = cell.is_some_and(dereth_physics::landdefs::is_outdoors);
                    // The converse crossing matters too. An indoor-origin door can own
                    // an outdoor shadow (the villa courtyard door does). Landscape drawing reaches that
                    // landcell's shadow list before portal traversal clears depth, regardless of its origin.
                    let outdoor_shadow = || {
                        handle
                            .and_then(|h| ws.character.as_ref().and_then(|c| c.world.get(h)))
                            .is_some_and(|body| {
                                body.shadow_objects.iter().any(|shadow| {
                                    shadow.cell_present
                                        && dereth_physics::landdefs::is_outdoors(shadow.cell_id)
                                })
                            })
                    };
                    match phase {
                        ObjectPhase::All => {
                            if outdoors {
                                (!viewer_inside).then_some(true)
                            } else {
                                interior_seen(cell, handle).then_some(false)
                            }
                        }
                        ObjectPhase::Outdoors => {
                            if outdoors || outdoor_shadow() {
                                Some(true)
                            } else {
                                interior_seen(cell, handle).then_some(false)
                            }
                        }
                        ObjectPhase::Interior => interior_seen(cell, handle).then_some(false),
                    }
                };
            // --- the server's objects -----------------------------------------------------
            // The same walk as the character below, once per object.
            // Their part frames were placed in `advance_objects`.
            //
            // **Three phases**, because the client's path has three:
            //
            //   1. **collect** every part that will draw, with its viewer distance beside it;
            //   2. **sort** with the stable part insertion sort, descending by that distance,
            //      farthest first;
            //   3. **submit**, classifying each subset through the alpha-delay mask and deferring
            //      the non-opaque ones to `AlphaLists`, then draining that.
            //
            // Phase 1 is the only place the two guards live: `SceneObject::drawn`
            // (a held object whose holder refused the attachment has no place in the world, exactly
            // as a refused parent assignment leaves it, so it is not submitted rather than
            // submitted at the identity), and `no_draw || meshes.is_empty()` is
            // the physics-part draw routine's hidden-bit and missing-geometry checks.
            let object_drivers: Vec<_> = ws
                .objects
                .iter()
                .map(|(id, o)| (*id, o, o.sim.driver.borrow()))
                .collect();
            let mut subs: Vec<PartSubmission<'_>> = Vec::new();
            for (id, o, driver) in &object_drivers {
                if !o.drawn {
                    continue;
                }
                // Which of cell drawing's two object passes this object belongs to.
                let draw_cell = ws.object_draw_cell(o);
                let Some(outdoors) = wanted_light(draw_cell, o.sim.physics_handle) else {
                    continue;
                };
                let d = self.object_draw(*id);
                for (i, pl) in d.meshes.iter().enumerate() {
                    let Some(part) = driver.part_array.parts.get(i) else {
                        continue;
                    };
                    // `gfxobj[deg_level]`, the level `refresh_part_levels` chose for
                    // this part this frame -- not the near-band bake.
                    let meshes = pl.at(d.part_levels.get(i).copied().unwrap_or(0));
                    if part.no_draw() || meshes.is_empty() {
                        continue;
                    }
                    subs.push(PartSubmission {
                        object: Some(*id),
                        index: i,
                        part,
                        outdoors,
                        before_depth_clear: phase == ObjectPhase::Outdoors,
                        cell: draw_cell,
                        // The level's own sphere, chosen by the same index that chose
                        // `meshes` two lines above.
                        drawing_sphere: pl
                            .drawing_sphere_at(d.part_levels.get(i).copied().unwrap_or(0)),
                        // The draw-position frame, which `refresh_part_levels` filled.
                        draw_pos: d.part_draw_pos.get(i).copied().unwrap_or(part.pos),
                        // The viewer distance, out of the same call.
                        cypt: d.part_cypt.get(i).copied().unwrap_or(0.0),
                        meshes,
                    });
                }
            }

            // --- the character ------------------------------------------------------------
            // Character drawing walks the part array and draws each part at the position filled
            // from this frame's animation frame. `no_draw` is a per-part flag an animation hook
            // can set. Each part also carries the clone that material setup creates,
            // which the material-binding step installs before the mesh goes down. [`draw_part`]
            // is that one call, and
            // `material` is [`SceneConfig::material_translucency`], passed in by the caller so
            // that both of a split frame's passes read one value.
            //
            // The `Ref` guard is hoisted out of the loop because a `PartSubmission` borrows the
            // part out of it and must outlive the sort and the flush below.
            // The body goes with the cell it is standing in, exactly as any other
            // physics body does: outdoors it is one of a land cell's objects, so the outdoor cell walk
            // has already drawn it through the opening; indoors the interior cell walk does so.
            // Drawing it anywhere else makes the body disappear until the camera makes it all the
            // way outside: the body walks out of the doorway and is stamped away.
            let body_light = wanted_light(
                ws.character.as_ref().map(|c| c.position().cell),
                ws.character.as_ref().map(|c| c.handle),
            );
            let body = body_light
                .and_then(|outdoors| ws.character.as_ref().map(|c| (c.driver(), outdoors)));
            if let Some((driver, outdoors)) = body.as_ref() {
                for (i, pl) in self.character_parts.iter().enumerate() {
                    let Some(part) = driver.part_array.parts.get(i) else {
                        continue;
                    };
                    // The local body's level is always 0 -- the viewer-distance update
                    // exempts `player_iid` -- but it is read rather than assumed, so that the
                    // exemption is one value in one place.
                    let meshes = pl.at(self.character_part_levels.get(i).copied().unwrap_or(0));
                    if part.no_draw() || meshes.is_empty() {
                        continue;
                    }
                    subs.push(PartSubmission {
                        object: None,
                        index: i,
                        part,
                        outdoors: *outdoors,
                        before_depth_clear: phase == ObjectPhase::Outdoors,
                        cell: ws.character.as_ref().map(|c| c.position().cell),
                        // As above. The local body's level is always 0.
                        drawing_sphere: pl.drawing_sphere_at(
                            self.character_part_levels.get(i).copied().unwrap_or(0),
                        ),
                        // As above.
                        draw_pos: self
                            .character_part_draw_pos
                            .get(i)
                            .copied()
                            .unwrap_or(part.pos),
                        cypt: self.character_part_cypt.get(i).copied().unwrap_or(0.0),
                        meshes,
                    });
                }
            }

            // Phase 2 — stable depth sorting. Without it the
            // order is `BTreeMap<ObjectId, _>` order then part order, i.e. the order the *server*
            // happened to hand out ids in, which has nothing to do with depth. It goes through
            // `dereth_world_render`'s own transcription rather than a `sort_by` here because the
            // client's sort is an **insertion** sort and therefore stable: two parts at exactly
            // the same distance keep registration order, and that is what decides which of two
            // coplanar translucent surfaces wins.
            if self.cfg.part_depth_sort {
                dereth_world_render::objects::parts::insertion_sort_by_cypt_key(&mut subs, |s| {
                    s.cypt
                });
            }

            // Phase 3 -- the mesh renderer's per-subset
            // classification and the alpha-list append.
            let mut pass = PartPass::default();
            // Each part's light set, chosen once per submission and reused by the
            // alpha flush -- the alpha-list flush re-installs the matrix and material it recorded,
            // and the lights are whatever active-light enabling left in the slots, which for a
            // deferred subset is the set its own inner mesh draw selected.
            let sets: Vec<Option<Vec<D3dLight>>> = subs
                .iter()
                .map(|s| {
                    self.cfg
                        .object_lighting
                        .then(|| self.submission_light_set(s))
                })
                .collect();
            // The watched id, read once for the loop -- the draw tests it
            // against 0 *before* comparing ids, so a frame with nothing selected does nothing here.
            let watched = self.viewcone_check_object_id.get();
            // `s.object` is `None` for the local body, and that is **not**
            // the null-object-id counterpart: the player's physics body carries his id, and
            // viewer-distance updating compares it against the player's id,
            // so his parts answer the draw's id compare like any other object's. The player can be
            // his own selection — a click on his own body selects him — and the latch has to come
            // back up when he draws, or the next object-range-exit handler takes its
            // deselect arm. `Character::object_id` is here.
            let body_id = ws.character.as_ref().map(|c| c.object_id().0);
            // **The per-object frustum test.**
            //
            // View setup installs one full-screen polygon and writes the viewer position;
            // together they
            // are the portal view's `1 +` point-count planes for every mesh
            // drawn outside a portal list, which is every mesh on this path. The polygon is the
            // client's own quad and the planes are view copying's own tail, so the cone is retail's
            // computation and not a frustum reconstructed from the view-projection matrix.
            let eye = self.eye_transform(ws, view);
            let near = self.viewer_near_plane(ws);
            let cone_view = dereth_world_render::cells::clip::ViewPoly::full_screen(
                eye.width, eye.height, &eye,
            );
            let mut cone = ObjectConeStats::default();
            // `Render.MultiPassAlpha`, read once for the loop because it is a
            // render preference and not a per-part decision.
            let multi_pass_alpha = self.cfg.render.multi_pass_alpha;
            // The particles, prepared (viewer distance, degrade level, draw frame, lights) and
            // already far to near, then merged into the parts' order by that distance.
            //
            // One particle's `D3DLIGHT9` set: particle drawing hands the particle to mesh
            // drawing, writes the placed, scaled drawing sphere into the local object centre and
            // radius, and object-light minimization (the inner mesh draw, under sunlight use 0)
            // reads exactly those. So the set is chosen per particle, at the particle, by the
            // same [`Self::object_light_set`] every animated part uses.
            let particle_lights =
                |c: Vec3, r: f32, outdoors: bool| self.object_light_set(c, r, outdoors);
            let ready = if particles.is_empty() {
                Vec::new()
            } else {
                crate::particles::prepare(
                    &self.particle_gfx,
                    ws.camera.position,
                    particles,
                    // The governor's outputs; an emitter's particle object shares at its
                    // particle distance.
                    &self.degrade.governor.level(),
                    // Live degrade inputs, so that `get_degrade`'s thresholds slide with the
                    // measured frame rate instead of sitting on `ideal_dist`.
                    &self.degrade_globals(ws),
                    // `minimize_object_lighting` per particle.
                    self.cfg
                        .object_lighting
                        .then_some(&particle_lights as crate::particles::ParticleLightSet<'_>),
                    particle_stats,
                )
            };
            particle_stats.meshes = self.particle_gfx.len();
            let particles_lit = self.cfg.object_lighting;
            let sub_cypts: Vec<f32> = subs.iter().map(|s| s.cypt).collect();
            let ready_cypts: Vec<f32> = ready.iter().map(|r| r.cypt).collect();
            let order =
                dereth_world_render::objects::parts::merge_far_to_near(&sub_cypts, &ready_cypts);
            for slot in order {
                let index = match slot {
                    dereth_world_render::objects::parts::Merged::First(i) => i,
                    dereth_world_render::objects::parts::Merged::Second(p) => {
                        let r = &ready[p];
                        let Some(gfx) = self.particle_gfx.get(r.gfx) else {
                            continue;
                        };
                        for (j, m) in gfx.meshes.iter().enumerate() {
                            let passes = if self.cfg.part_alpha_lists {
                                dereth_world_render::objects::draw::classify_subset_passes(
                                    m.subset_mask,
                                    dereth_world_render::consts::S_ALPHA_DELAY_MASK,
                                    multi_pass_alpha,
                                )
                            } else {
                                dereth_world_render::objects::draw::SubsetPasses {
                                    list: None,
                                    immediate: true,
                                    multipass: false,
                                }
                            };
                            if let Some(list) = passes.list {
                                // LINT-OK: an index into this frame's own particle queue, capped
                                // by `AlphaLists` itself. Not a float conversion.
                                #[allow(clippy::cast_possible_truncation)]
                                let handle = PARTICLE_ENTRY | pass.particle_queued.len() as u32;
                                if pass.lists.push(
                                    list,
                                    dereth_world_render::objects::alpha::AlphaEntry {
                                        mesh: dereth_primitives::MeshHandle(handle),
                                        // LINT-OK: a subset index within one emitter mesh.
                                        #[allow(clippy::cast_possible_truncation)]
                                        surface_num: j as u32,
                                        texture: None,
                                        first_of_kind: false,
                                        world_matrix: Frame::default(),
                                        multipass: passes.multipass,
                                        range: 0..0,
                                    },
                                ) {
                                    pass.particle_queued.push((p, j));
                                    match list {
                                        dereth_world_render::objects::alpha::AlphaList::Clip => {
                                            pass.particle_clip += 1;
                                        }
                                        dereth_world_render::objects::alpha::AlphaList::Blend => {
                                            pass.particle_blend += 1;
                                        }
                                    }
                                }
                            }
                            if passes.immediate {
                                crate::particles::draw_one(
                                    gpu,
                                    per_frame,
                                    r,
                                    m,
                                    particles_lit,
                                    false,
                                    particle_stats,
                                )?;
                            }
                        }
                        continue;
                    }
                };
                let s = &subs[index];
                // The view cone for this part: the full-screen cone of
                // the null portal-list branch —
                // unless this is the pass cell drawing issues with the outside view as its portal list, in
                // which case it is the **portal** branch.
                // **When the pass is the per-cell object draw's, the cone is the
                // owning cell's own `portal_view`.**
                //
                // The cell-object pass installs the cell's newest portal view as the active portal
                // list immediately before calling the per-cell object draw,
                // so the parts of an interior cell's objects are tested against the polygons *that
                // cell* is seen through. An object in a cell the walk reached but which lies
                // outside the doorway it was reached through is therefore neither drawn nor
                // offered to the selection-ray test.
                //
                // An outdoor cell has no `portal_view`, and a cell the map does not name is one no
                // interior loop ran for; both fall back to the full-screen cone, which is the same
                // superset the unclipped traversal draws. `outside_views` wins where it applies, because
                // that pass runs under `&outside_view` rather than under any cell's.
                //
                // **An object that overlaps several cells is drawn by each of them.** It is
                // registered in every cell its body overlaps, and each reached cell draws its
                // registered parts under its own `portal_view`; the frame stamp lets a part go
                // down once however many cells draw it. So a part is in view when any reached
                // cell it is registered in sees it. A body standing just past an opening, with
                // the camera in the cell behind, is seen whole through the camera's own cell even
                // where the opening's polygon would cut its upper parts off.
                let here = s.cell.and_then(|c| cell_views.and_then(|m| m.get(&c.0)));
                let shadow_views: Vec<dereth_world_render::cells::clip::ViewPoly> =
                    match (outside_views, cell_views) {
                        (None, Some(m)) => {
                            let handle = match s.object {
                                Some(id) => ws.objects.get(&id).and_then(|o| o.sim.physics_handle),
                                None => ws.character.as_ref().map(|c| c.handle),
                            };
                            handle
                                .and_then(|h| ws.character.as_ref().and_then(|c| c.world.get(h)))
                                .map(|body| {
                                    let mut cells: Vec<u32> = body
                                        .shadow_objects
                                        .iter()
                                        .filter(|sh| sh.cell_present)
                                        .map(|sh| sh.cell_id.0)
                                        .filter(|c| Some(*c) != s.cell.map(|o| o.0))
                                        .collect();
                                    cells.sort_unstable();
                                    cells.dedup();
                                    cells
                                        .iter()
                                        .filter_map(|c| m.get(c))
                                        .flatten()
                                        .cloned()
                                        .collect()
                                })
                                .unwrap_or_default()
                        }
                        _ => Vec::new(),
                    };
                let joined: Vec<dereth_world_render::cells::clip::ViewPoly>;
                // A part whose own cell has no views keeps the full-screen cone below.
                let here = match here {
                    Some(own) if !shadow_views.is_empty() => {
                        joined = own.iter().cloned().chain(shadow_views).collect();
                        Some(joined.as_slice())
                    }
                    other => other.map(Vec::as_slice),
                };
                let status = match (outside_views, here) {
                    (Some(views), _) => self.part_cone_views(s, &near, views, &mut cone),
                    (None, Some(views)) if !views.is_empty() => {
                        self.part_cone_views(s, &near, views, &mut cone)
                    }
                    _ => self.part_cone(s, &near, &cone_view, &mut cone),
                };
                if status != dereth_world_render::objects::draw::MeshDrawStatus::InsideViewcone
                    && self.cfg.object_viewcone
                {
                    // Not drawn: mesh drawing returns `OutsideViewcone` without ever reaching
                    // the inner mesh draw. The index is still consumed, so `defer`'s handles and
                    // the flush's `subs.get(q.submission)` stay in step.
                    continue;
                }
                // **The offer to the selection ray, at exactly retail's point.**
                //
                // Both the portal and non-portal mesh-draw arms offer the object
                // to the selection ray **before** their internal mesh draw
                // and **after** view-cone checking. So the
                // eligible set is "submitted after phase, `NoDraw`, mesh and view-cone gates", and
                // that is this line: below the `continue` above and above `draw_part`.
                //
                // `s.object` is `None` for the local body;
                // `body_id` is what retail's physics-object id reads for him,
                // so he is entered under his own id.
                if let Some(seen) = self.frame_pick_candidates.borrow_mut().as_mut() {
                    if let Some(id) = s.object.or_else(|| body_id.map(ObjectId)) {
                        seen.insert(id);
                    }
                }
                // LINT-OK: an index into this frame's own submission list; the largest scene
                // measured here holds a four-digit number of parts. Not a float conversion.
                #[allow(clippy::cast_possible_truncation)]
                let defer = self.cfg.part_alpha_lists.then_some(index as u32);
                draw_part(
                    gpu,
                    per_frame,
                    s,
                    material,
                    defer,
                    &mut pass,
                    sets[index].as_deref(),
                    multi_pass_alpha,
                )?;
                // **The selected-object visibility latch.**
                //
                // When mesh drawing answers `InsideViewcone`, the watched id
                // `viewcone_check_object_id` is non-zero, and it equals the part's physics-object id
                // (0 when there is none), the selected-object-in-view flag is set to 1.
                //
                // It sits *after* the draw, per part, and all three of its tests are
                // transcribed. The first is the view-cone answer: `InsideViewcone` exactly when
                // the mesh's drawing sphere is not OUTSIDE, which is `status` above. Degenerating it
                // to the id compare alone can only make the latch rise **earlier**, never fail to
                // rise; a test drives an object submitted but outside the cone and reads the latch
                // on both arms.
                //
                // `s.object` is `None` for the local body; `body_id` above is what the client's
                // physics-object id reads for him (he *can* be his own selection).
                if status == dereth_world_render::objects::draw::MeshDrawStatus::InsideViewcone
                    && watched != 0
                    && s.object.map(|o| o.0).or(body_id) == Some(watched)
                {
                    self.selected_part_drawn.set(true);
                }
            }
            cone_out.tested += cone.tested;
            cone_out.outside += cone.outside;
            cone_out.no_sphere += cone.no_sphere;
            cone_out.culled += cone.culled;
            let mut stats = AlphaListStats {
                parts: subs.len(),
                clip: pass.lists.clip_len() - pass.particle_clip,
                blend: pass.lists.blend_len() - pass.particle_blend,
                dropped: pass.lists.dropped,
                immediate: pass.counts.immediate,
                flushed: 0,
                multipass: 0,
            };
            // Flushing the alpha list at 0.0 draws the clip list in insertion order, then the blend
            // list. `ready(0.0)` is always true and is *called* rather than assumed,
            // so the threshold has one statement (`AlphaLists::ready`) and not a second copy here.
            self.frame_alpha_order
                .borrow_mut()
                .push(AlphaDraw::FlushStart);
            let mut tail_drawn = false;
            if pass.lists.ready(0.0) {
                let clip_entries = pass.lists.clip_len();
                for (k, e) in pass.lists.take_draw_order().into_iter().enumerate() {
                    // The clip list is exhausted: the rest of it goes down before the first blend
                    // entry.
                    if k == clip_entries {
                        clip_tail(gpu)?;
                        tail_drawn = true;
                    }
                    // A particle's entry: drawn here, in its list and in its place in it, exactly
                    // as a part's is.
                    if e.mesh.0 & PARTICLE_ENTRY != 0 {
                        let Some(&(p, j)) = pass
                            .particle_queued
                            .get((e.mesh.0 & !PARTICLE_ENTRY) as usize)
                        else {
                            continue;
                        };
                        let r = &ready[p];
                        let Some(m) = self.particle_gfx.get(r.gfx).and_then(|g| g.meshes.get(j))
                        else {
                            continue;
                        };
                        crate::particles::draw_one(
                            gpu,
                            per_frame,
                            r,
                            m,
                            particles_lit,
                            e.multipass,
                            particle_stats,
                        )?;
                        let kind = if k >= clip_entries {
                            AlphaDraw::ParticleBlend
                        } else if e.multipass {
                            AlphaDraw::ParticleForced
                        } else {
                            AlphaDraw::ParticleClip
                        };
                        self.frame_alpha_order.borrow_mut().push(kind);
                        if kind == AlphaDraw::ParticleBlend {
                            self.frame_blend_order.borrow_mut().push((kind, r.cypt));
                        }
                        continue;
                    }
                    // The entry's `mesh` handle indexes `queued`, which is the push order; its
                    // `surface_num` is the client's own surface number and is *not* the lookup key,
                    // because two subsets of one part push two entries with different surface numbers
                    // and the same submission.
                    let Some(q) = pass.queued.get(e.mesh.0 as usize) else {
                        continue;
                    };
                    let Some(s) = subs.get(q.submission as usize) else {
                        continue;
                    };
                    let Some(m) = s.meshes.get(q.subset) else {
                        continue;
                    };
                    // The alpha-list flush re-installs the recorded matrix and material for an entry
                    // whose matrix-valid flag is set and lets the rest inherit; this re-installs both
                    // for **every** entry, which is a superset and produces the same device state
                    // for each draw. `first_of_kind` is still recorded, because it is what a test
                    // asserts the queueing against.
                    //
                    // An entry "Multiple Pass Alpha" queued is drawn with surface setup's
                    // force-alpha argument: blended, not alpha-tested, no depth write. Its first
                    // pass, the in-place one, already wrote depth wherever the alpha test passed,
                    // so under `LESS` this one lands only on the edge texels the test cut away.
                    submit_part_mesh(
                        gpu,
                        per_frame,
                        s.part,
                        &s.draw_pos,
                        m,
                        material,
                        sets.get(q.submission as usize).and_then(|x| x.as_deref()),
                        e.multipass,
                    )?;
                    // The trace is in **device submission order**, so a deferred subset is
                    // recorded here and not where it was queued: that is the whole observable.
                    pass.trace.push(PartSubsetDraw {
                        force_alpha: e.multipass,
                        ..q.probe
                    });
                    self.frame_alpha_order
                        .borrow_mut()
                        .push(if k < clip_entries {
                            if e.multipass {
                                AlphaDraw::PartForced
                            } else {
                                AlphaDraw::PartClip
                            }
                        } else {
                            AlphaDraw::PartBlend
                        });
                    if k >= clip_entries {
                        self.frame_blend_order
                            .borrow_mut()
                            .push((AlphaDraw::PartBlend, s.cypt));
                    }
                    stats.flushed += 1;
                    stats.multipass += usize::from(e.multipass);
                }
            }
            if !tail_drawn {
                clip_tail(gpu)?;
            }
            alpha_out.parts += stats.parts;
            alpha_out.clip += stats.clip;
            alpha_out.blend += stats.blend;
            alpha_out.dropped += stats.dropped;
            alpha_out.immediate += stats.immediate;
            alpha_out.flushed += stats.flushed;
            alpha_out.multipass += stats.multipass;
            counts_out.0 += pass.counts.parts;
            counts_out.1 += pass.counts.material;
            trace_out.extend(pass.trace);
            Ok(())
        }

        /// Every live emitter in the scene, with **both** cut-off distances: the one
        /// emitter setup stores for it and the one this build
        /// actually hands the should-draw-particles test.
        ///
        /// It exists because the two are not the same number and the difference is not visible
        /// from any counter this scene already had. Nothing in the update path is duplicated here:
        /// the distances come from the same [`manager_degrade_distance`] and
        /// [`crate::particles::ParticleGeometry::max_degrade_distance`] the update uses, and the
        /// viewer distance from the same origin arithmetic.
        #[must_use]
        pub fn emitter_degrade_probe(&self, ws: &WorldState) -> Vec<EmitterDegrade> {
            let viewer = detail_viewer(ws);
            let shift = self.block_shift(ws);
            let mut out = Vec::new();
            let mut push = |owner: EmitterOwner,
                            origin: Vec3,
                            m: &dereth_animation::ParticleManager| {
                let cypt = origin.sub(viewer).mag2().sqrt();
                let manager_distance = manager_degrade_distance(&self.particle_gfx, m);
                for e in m.iter() {
                    let own_distance = self.particle_gfx.max_degrade_distance(e.info.hw_gfxobj_id);
                    out.push(EmitterDegrade {
                        owner,
                        origin,
                        emitter: e.id,
                        gfxobj: e.info.hw_gfxobj_id,
                        own_distance,
                        manager_distance,
                        cypt,
                        live: e.live().count(),
                    });
                }
            };
            for (bi, block) in self.blocks.values().enumerate() {
                for (hi, h) in block.hosts.iter().enumerate() {
                    push(
                        EmitterOwner::Placement(bi, hi),
                        h.frame.origin.add(shift),
                        &h.driver.particles,
                    );
                }
            }
            for (id, o) in &ws.objects {
                push(
                    EmitterOwner::Object(*id),
                    o.frame.origin,
                    &o.sim.driver.borrow().particles,
                );
            }
            if let Some(c) = ws.character.as_ref() {
                let origin = c.render_frame().origin;
                push(EmitterOwner::Body, origin, &c.driver().particles);
            }
            out
        }

        /// Every emitter host's origin in the renderer's space, for the tests.
        #[must_use]
        pub fn emitter_host_origins(&self, ws: &WorldState) -> Vec<Vec3> {
            let shift = self.block_shift(ws);
            self.blocks
                .values()
                .flat_map(|b| b.hosts.iter().map(move |h| h.frame.origin.add(shift)))
                .collect()
        }

        /// What the last [`Self::draw`] drew, for the counters and the tests. `draw` takes `&self`
        /// — it runs inside the frame bracket, where nothing may mutate the scene — so its two
        /// counters live in a `Cell` rather than in [`SceneStats`].
        #[must_use]
        pub fn drawn_particles(&self) -> crate::particles::ParticleStats {
            self.frame_particles.get()
        }

        /// Same bracket: animated parts submitted and parts drawn through a cloned
        /// material's alpha for the last [`Self::draw`].
        ///
        /// The denominator is not decoration. A pixel differential over this channel that reads
        /// zero is only a negative if some part was actually submitted *and* carried a material;
        /// with either count at zero the run measured nothing.
        #[must_use]
        pub fn drawn_material_parts(&self) -> (u32, u32) {
            self.frame_material_parts.get()
        }

        /// Same bracket: the polygon renderer's two deferred lists for the moving
        /// objects of the last [`Self::draw`].
        ///
        /// Every field is a denominator for one of the others, which is the point of publishing
        /// five numbers rather than a bool: `clip + blend == flushed` says the flush drained what
        /// the classification queued, `immediate` says how much of the scene never went near the
        /// lists, `parts` says whether anything was submitted at all, and `dropped` distinguishes
        /// *"the 3 000-entry cap threw this subset away"* -- which is a shipped behaviour, not a
        /// bug -- from *"nothing was ever queued"*.
        #[must_use]
        pub fn drawn_alpha_lists(&self) -> AlphaListStats {
            self.frame_alpha_lists.get()
        }

        /// Same bracket: what the *landscape's* alpha flush drew on
        /// the last [`Self::draw`], and in what order. See [`LandscapeAlphaStats`].
        #[must_use]
        pub fn drawn_landscape_alpha(&self) -> LandscapeAlphaStats {
            self.frame_landscape_alpha.get()
        }

        /// Same bracket: every alpha-list draw of the last [`Self::draw`] in device order, by
        /// kind, with an [`AlphaDraw::FlushStart`] where each flush begins. See [`AlphaDraw`].
        #[must_use]
        pub fn drawn_alpha_order(&self) -> Vec<AlphaDraw> {
            self.frame_alpha_order.borrow().clone()
        }

        /// Same bracket: the object pass's blend-list draws of the last [`Self::draw`], parts and
        /// particles, in device order with the viewer distance each was sorted by.
        #[must_use]
        pub fn drawn_blend_order(&self) -> Vec<(AlphaDraw, f32)> {
            self.frame_blend_order.borrow().clone()
        }

        /// What did on the last
        /// [`Self::draw`]: how many parts it was asked about, how many it rejected, how many it
        /// could not be asked about, and how many draws were actually skipped.
        ///
        /// `tested == 0` means the cull did not run on anything and every other number here is
        /// uninformative -- which is the distinction a bare `outside` count cannot make.
        #[must_use]
        pub fn drawn_object_cone(&self) -> ObjectConeStats {
            self.frame_object_cone.get()
        }

        /// Same bracket: every subset the last [`Self::draw`] put on
        /// the device, in the order it did, with the sort key and the classification that decided
        /// where it went.
        #[must_use]
        pub fn drawn_part_order(&self) -> Vec<PartSubsetDraw> {
            self.frame_part_order.borrow().clone()
        }

        /// Every **interior** cell the last [`Self::draw`] walked,
        /// or `None` if no draw has run — so that a hover does not name objects behind interior
        /// walls.
        ///
        /// Cell traversal draws a cell's objects once per entry
        /// of the cell draw list, and a cell holds only the objects
        /// registered in *it*. So an object standing in an interior cell
        /// the portal traversal never reached is never offered to part drawing and
        /// therefore never offered to the selection ray: **only objects that are actually drawn
        /// can be picked**, and the wall the player is looking at is what makes the difference.
        /// [`dereth_client_runtime::pick::WorldPicker::draw_no_blit`] is the consumer.
        ///
        /// The set is the draw's, not a re-derivation, for the reason
        /// [`Self::frame_part_order`] gives: a probe that re-ran the traversal would agree with
        /// itself whatever the draw did. The cost is **one frame of lag** — `App::frame` runs
        /// `interaction_use_time`'s pick before the world draws — which is the same lag the camera
        /// sweep carries (see `Self::viewpoint`) and is bounded by one frame of portal travel.
        #[must_use]
        pub fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
            self.frame_drawn_cells.borrow().clone()
        }

        /// Every object the last [`Self::draw`] **offered to the selection ray** — so that,
        /// indoors, a hover does not name an exterior object: one whose parts
        /// survived phase, `NoDraw`, mesh, and view-cone selection and reached the selection-ray
        /// offer immediately before mesh drawing.
        ///
        /// [`Self::drawn_cells`] cannot answer this. It is a **cell** set, and the case that
        /// matters is an **outdoor** cell whose objects were skipped not by the cell walk but by
        /// the absent walk: normal indoor rendering never calls the landscape directly, and cell
        /// rendering — the only indoor path to it — skips the whole outdoor block when
        /// `outside_view.view_count == 0`.
        ///
        /// `None` is "no draw has run", not "nothing was offered", and restricts nothing; an empty
        /// set is a real answer. The consumer is
        /// [`dereth_client_runtime::pick::WorldPicker::draw_no_blit`], and it carries the same **one frame of
        /// lag** [`Self::drawn_cells`] documents, for the same reason.
        #[must_use]
        pub fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
            self.frame_pick_candidates.borrow().clone()
        }

        /// The last [`Self::draw`]'s polygons, as screen
        /// outlines in pixels with y down.
        ///
        /// One copied view per exterior portal through which portal clipping found an
        /// opening. Mesh drawing's portal arm cone-tests the outdoor pass against these views.
        ///
        /// **Empty is three different states and the caller must not conflate them**: an outdoor
        /// frame, an indoor frame with no opening on screen, and the unclipped traversal, which has
        /// an `outside_view_count` but no polygons. Use
        /// [`Self::indoor_outside_view_count`] for the count.
        #[must_use]
        pub fn outside_view_polys(&self) -> Vec<Vec<(f32, f32)>> {
            self.frame_outside_views
                .borrow()
                .iter()
                .map(|v| v.points.clone())
                .collect()
        }

        /// One interior cell's newest portal view from the last
        /// [`Self::draw`], as screen outlines in pixels with y down.
        ///
        /// These are the polygons retail installs as the portal list
        /// before that cell's per-cell object draw, and therefore what every part of
        /// its objects is coned against before the mesh's ray offer. Empty is a
        /// cell the walk did not reach, an outdoor frame, or the unclipped traversal.
        ///
        /// `(indoor stamps issued, opening/view pairs the clip emptied)` for the
        /// last [`Self::draw`]. See [`Self::frame_portal_stamps`].
        #[must_use]
        pub fn drawn_portal_stamps(&self) -> (u64, u64) {
            self.frame_portal_stamps.get()
        }

        /// One interior cell's newest portal view.
        #[must_use]
        pub fn cell_view_polys(&self, cell: CellId) -> Vec<Vec<(f32, f32)>> {
            self.frame_cell_views
                .borrow()
                .get(&cell.0)
                .map(|v| v.iter().map(|p| p.points.clone()).collect())
                .unwrap_or_default()
        }

        /// Draw an interior viewpoint's frame:
        /// the portal traversal and the cells it reaches.
        ///
        /// The traversal, its ordering and the step sequence are all the world-render crate's
        /// ([`dereth_world_render::cells::portal_view`]); what is here is the per-frame *input* —
        /// the two viewpoint facts used to initialize each portal — and the draw itself.
        ///
        /// **`ZClear` and `PortalDepthStamps` are performed, clip or no clip.** They are what
        /// stops the outdoor pass's **depth** from standing in front of the interior, and the
        /// clip has nothing to do with it. `Clear(4)` clears the depth buffer over the whole
        /// viewport whether the pass that filled it was clipped or not, and the openings are
        /// stamped back from the cells' own decoded portal polygons.
        ///
        /// Without them, standing in a Holtburg house, the terrain the
        /// house is dug into wins the depth test against the basement stairwell and the ground is
        /// painted over the stairs. See [`SceneConfig::indoor_z_clear`].
        ///
        /// **`OutdoorsThroughPortals` can be issued unclipped.** The clip is
        /// formed from screen-space polygons, which need the projection matrix; unclipped draws a
        /// superset of what retail draws through the opening and is a declared deviation rather
        /// than the client's behaviour. The Z clear makes that
        /// superset *invisible* wherever the interior covers it, which is most of it.
        ///
        /// **The outdoor pass is issued from here, not unconditionally.** Running it on every
        /// frame for every viewer is not a superset of retail — for an indoor viewer whose
        /// traversal reaches no outdoor portal, retail draws **none** of it — and it puts a flat
        /// light backdrop behind every gap in a dungeon's cells. The step is emitted by
        /// `indoor_steps` on `outside_view.view_count != 0` and drawn here.
        ///
        /// `after_outdoors` is run immediately after [`IndoorStep::OutdoorsThroughPortals`] and
        /// before [`IndoorStep::ZClear`] — the gap where retail has
        /// already drawn the outdoor blocks *and their cells' objects* through the openings and
        /// has not yet cleared the depth buffer or stamped the openings back into it. See
        /// [`Self::draw_object_pass`].
        ///
        /// [`IndoorStep::OutdoorsThroughPortals`]: dereth_world_render::cells::portal_view::IndoorStep::OutdoorsThroughPortals
        /// [`IndoorStep::ZClear`]: dereth_world_render::cells::portal_view::IndoorStep::ZClear
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        fn draw_inside(
            &self,
            ws: &WorldState,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
            sky_per_frame: &PerFrameConstants,
            interior_cells: &mut BTreeSet<u32>,
            after_outdoors: &mut dyn FnMut(
                &mut Gpu,
                &BTreeSet<u32>,
                &[dereth_world_render::cells::clip::ViewPoly],
            ) -> Result<(), RenderError>,
        ) -> Result<(), RenderError> {
            use dereth_render::pso::portal_stamp_mask;
            use dereth_world_render::cells::clip::{poly_clip_finish, ScreenPoint, ViewPoly};
            use dereth_world_render::cells::portal_view::{
                construct_view, construct_view_clipped, indoor_steps, IndoorStep,
            };
            let Some(start) = ws.viewer_cell() else {
                return Ok(());
            };

            let (cells, placed) = self.traversal_cells(ws);
            if !cells.contains_key(&start.0) {
                return Ok(());
            }

            // The two repair steps need the projection and viewport the frame is
            // drawn with. Both are built from the same
            // [`Self::view_params`] `per_frame` came from, for the reason
            // [`Self::eye_transform`] gives.
            let (vw, vh) = gpu.size();
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent
            let (fw, fh) = (vw as f32, vh as f32);
            let vp = self.view_params(ws, vw, vh);
            let view_proj = dereth_render::camera::projection(&vp) * vp.view;
            let eye = self.eye_transform(ws, &vp);
            let full = ViewPoly::full_screen(fw, fh, &eye);

            // Projecting a cell-portal polygon with the transformed-vertex path uses the same closure
            // [`Self::draw_building_interiors`] hands `construct_view_clipped`, because it is the
            // same question: the vertices are in the owning cell's space and the draw is in that
            // cell's *block*'s frame, and a cell reached through a portal may belong to another
            // block than the viewer's.
            let project = |c: CellId, p: usize| -> Vec<ScreenPoint> {
                let Some((cell, origin)) = placed.get(&c.0) else {
                    return Vec::new();
                };
                let Some(portal) = cell.portals.get(p) else {
                    return Vec::new();
                };
                let world = swap_zup_to_d3d()
                    * frame_matrix(&Frame::new(
                        Vec3::new(origin.0, origin.1, 0.0),
                        Quat::IDENTITY,
                    ));
                portal
                    .vertices
                    .iter()
                    .map(|v| {
                        let bl = dereth_physics::math::localtoglobal(&cell.frame, *v);
                        let q = view_proj * world * glam::Vec4::new(bl.x, bl.y, bl.z, 1.0);
                        ScreenPoint::from_clip([q.x, q.y, q.z, q.w], fw, fh)
                    })
                    .collect()
            };

            // **Portal clipping is performed on the indoor path too.**
            //
            // View construction's `clip` is a `bool`. Stubbed to "visible", the
            // traversal walks a **superset** of the cells, in the same order, and
            // `outside_view.view_count` counts every exterior portal the chain
            // reached rather than every one still visible on the screen. So an interior whose
            // portal graph reaches the outdoors *anywhere* would run the outdoor pass on every
            // frame, at full screen, even with the camera facing a wall. Retail refuses a portal
            // whose clip leaves nothing before attempting to copy the view.
            //
            // `SceneConfig::portal_clip` is the same switch the **outdoor** building path already
            // takes this branch under, so the two halves of the portal machinery cannot disagree
            // about whether clipping is on. Off is the unclipped superset, kept verbatim.
            let view = if self.cfg.portal_clip {
                construct_view_clipped(&cells, start, None, None, &full, &eye, &project)
            } else {
                construct_view(&cells, start, &|_, _| true)
            };
            self.frame_outside_views
                .borrow_mut()
                .clone_from(&view.outside_views);
            self.frame_outside_view_count
                .set(Some(view.outside_view_count));
            // Extended rather than assigned: with `SceneConfig::outside_view_gate` off an indoor
            // frame runs `draw_lscape` — and therefore `draw_building_interiors`' own publication —
            // *before* this line, and a `clone_from` would drop it.
            {
                let mut published = self.frame_cell_views.borrow_mut();
                for (id, polys) in &view.cell_views {
                    published
                        .entry(*id)
                        .or_default()
                        .extend(polys.iter().cloned());
                }
            }
            interior_cells.extend(view.cell_draw_list.iter().map(|cell| cell.0));
            // Sunlight use 0 for the whole of cell drawing: every env cell
            // takes `minimize_envcell_lighting`'s set, computed once because the dynamic pool
            // does not change inside a frame.
            let cell_set = self.cfg.object_lighting.then(|| self.envcell_light_set());
            // Environment-cell drawing installs the environment detail surface before every
            // cell mesh it draws.
            let env_detail =
                self.current_detail(dereth_world_render::detail::DetailClass::Environment);
            // `cell_draw_list` is the set cell drawing draws and therefore the set
            // the object draw routine is ever offered indoors, which is the set the pick may consider.
            // Recorded here, from the walk itself, rather than re-derived later.
            if let Some(seen) = self.frame_drawn_cells.borrow_mut().as_mut() {
                seen.extend(view.cell_draw_list.iter().map(|c| c.0));
            }
            for step in indoor_steps(&view) {
                // Cell drawing runs **two** loops over `cell_draw_list`, each backwards and so each
                // far to near: the first draws every cell's mesh, the second calls
                // the per-cell object draw on every cell, which is that cell's objects.
                // So a dungeon's whole geometry is laid down before any of its furniture.
                match step {
                    // Cell rendering's first step, and the whole
                    // reason an indoor frame ever contains sky, terrain or scenery: a non-empty
                    // `outside_view` becomes the active portal list for drawing the landscape.
                    // Portal clipping fills `outside_view`
                    // by copying a view once per portal whose other-cell id is `0xFFFFFFFF` that
                    // clipping finds visible, and [`indoor_steps`] emits this step only when
                    // that count is non-zero. A dungeon with no outdoor portal in view therefore
                    // gets **no** outdoor pass at all, and the black clear stands behind
                    // whatever its cells do not cover.
                    //
                    // The pass itself is issued **unclipped**, a standing deviation. Unclipped is a
                    // superset of what retail draws
                    // here, so nothing retail drew is missing; what the gate removes is the case
                    // where retail draws **nothing**.
                    IndoorStep::OutdoorsThroughPortals => {
                        let building_cells =
                            self.draw_lscape(ws, gpu, per_frame, sky_per_frame, false)?;
                        // Landscape drawing is not only the terrain: it
                        // walks `block_draw_list` and calls the device's block-draw operation on
                        // each in-view landblock, and a landblock draws
                        // its own landcells' **objects**. So every object standing in an outdoor
                        // cell is already on the screen before `Clear(4)` and
                        // before the openings are stamped back. Drawing every object
                        // after `draw_inside` returned would make an object past the doorway lose
                        // `ZFunc::Less` against the stamp's own depth and vanish.
                        // The outside view is still installed as the portal list
                        // at this point in cell drawing, so every mesh this
                        // pass submits goes down mesh drawing's **portal** arm and is
                        // cone-tested against those polygons before the mesh's ray offer.
                        // `outside_views` is empty on the unclipped traversal, which is the
                        // full-screen superset, and the callee treats that as "no list".
                        after_outdoors(gpu, &building_cells, &view.outside_views)?;
                    }
                    // Flush the alpha list with depth 0.0 and increment
                    // the frame stamp, the two lines between landscape drawing and the Z clear.
                    // The queue is real: `draw_lscape` fills it and does not drain it, so this
                    // step is the alpha
                    // flush — the landscape's translucency drawn after the outdoor objects
                    // `after_outdoors` has just put on the screen and before the depth clear below
                    // throws that depth away. The frame stamp is still
                    // the cell's drawn-this-frame counter, which `draw_inside` does not keep;
                    // it is named rather than skipped silently.
                    IndoorStep::FlushBeforeClear => {
                        self.flush_pending_alpha_list(gpu, per_frame)?;
                    }
                    // The device clear, which affects
                    // the **depth buffer only**. The
                    // landscape's colour stays where the outdoor pass put it; its depth does not, so
                    // the interior below draws over it instead of losing the depth test to a
                    // hillside the room is dug into.
                    //
                    // Retail's guard is "the Z-clear flag is set, or portals were drawn". That flag
                    // is **0** in retail and
                    // the drawn-portal count is incremented by the stamps *below* and read-and-cleared
                    // here, so the guard is "the previous frame stamped at least one opening" —
                    // true on every frame after the first at any station that reaches this step at
                    // all, because reaching it means `outside_view.view_count != 0`. This build
                    // does not carry the one-frame lag: a first frame that cleared nothing would be
                    // a single-frame flash of exactly the defect, and nothing observes the counter.
                    IndoorStep::ZClear => {
                        if self.cfg.indoor_z_clear {
                            gpu.clear_depth(vp.viewport);
                        }
                    }
                    // **The half that keeps the land in the window.**
                    //
                    // Retail walks the cell draw list far to near. Each cell with a drawing BSP pushes
                    // its own frame at position level 3; for each of its views it installs that view
                    // and draws, with the flag `false`, the portal polygon of every portal whose
                    // other-cell id is -1 (outdoors); then it pops the frame.
                    //
                    // `false` selects retail's mask **6**, beside the building half's 7: bit 1 forces
                    // the vertex alpha to 0 (no colour), bit 2 enables the depth write, and bit 0 is
                    // **clear**, so the depth written is the opening's own `z/w` rather than the far
                    // constant the building half stamps. That is the difference between the two
                    // callers and it is the whole point here — the opening gets *its* depth back
                    // into the freshly cleared buffer, so an interior polygon farther away than the
                    // window does not paint over the land seen through it, and one nearer still
                    // does.
                    //
                    // The level-3 push of the cell's position is the cell's own frame: the portal polygons are
                    // in cell space and the block push is the batch's, exactly as
                    // [`Self::draw_building_interiors`]' `project` closure has it.
                    // The ±12 rejection is the portal-polygon draw's own four flags and is
                    // [`is_dummy_portal`]. `poly_clip_finish` is issued **without** portal clipping's
                    // sidedness reversal, because the portal-polygon draw does not perform one:
                    // it transforms and clips, and nothing else.
                    //
                    // **The view it clips against is the cell's own, one at a time.**
                    //
                    // Portal-polygon drawing clips against the currently installed view, and
                    // the loop that calls it installs one view per iteration: it selects the cell
                    // view, accepts only an outdoors portal (other-cell id -1), and then draws its
                    // depth stamp. Selecting the cell view installs view `v` of the cell's newest
                    // portal-view entry.
                    // So an opening is stamped **once per view polygon of its own cell**, each time
                    // clipped to that polygon — not once against the screen. A cell reached through
                    // a doorway therefore
                    // stamps only the part of its window that is visible through that doorway.
                    //
                    // `frame_cell_views` is empty on the unclipped traversal, and the fallback is
                    // the full-screen clip, which is the unclipped superset.
                    IndoorStep::PortalDepthStamps => {
                        if self.cfg.indoor_z_clear && self.cfg.portal_depth_stamp {
                            let cell_views = self.frame_cell_views.borrow();
                            for id in view.draw_order() {
                                let Some((cell, origin)) = placed.get(&id.0) else {
                                    continue;
                                };
                                let world = swap_zup_to_d3d()
                                    * frame_matrix(&Frame::new(
                                        Vec3::new(origin.0, origin.1, 0.0),
                                        Quat::IDENTITY,
                                    ));
                                let here: &[ViewPoly] = cell_views
                                    .get(&id.0)
                                    .map_or(std::slice::from_ref(&full), Vec::as_slice);
                                for p in &cell.portals {
                                    if p.other_cell_id != 0xFFFF_FFFF
                                        || is_dummy_portal(&p.vertices)
                                    {
                                        continue;
                                    }
                                    let screen: Vec<ScreenPoint> = p
                                        .vertices
                                        .iter()
                                        .map(|v| {
                                            let bl = dereth_physics::math::localtoglobal(
                                                &cell.frame,
                                                *v,
                                            );
                                            let q = view_proj
                                                * world
                                                * glam::Vec4::new(bl.x, bl.y, bl.z, 1.0);
                                            ScreenPoint::from_clip([q.x, q.y, q.z, q.w], fw, fh)
                                        })
                                        .collect();
                                    for v in here {
                                        let clipped = poly_clip_finish(&screen, v);
                                        // `poly_clip_finish` leaving fewer than three vertices is
                                        // the opening being wholly outside this view; retail's
                                        // portal-polygon draw has nothing to submit then.
                                        if clipped.len() < 3 {
                                            let (a, b) = self.frame_portal_stamps.get();
                                            self.frame_portal_stamps.set((a, b + 1));
                                            continue;
                                        }
                                        {
                                            let (a, b) = self.frame_portal_stamps.get();
                                            self.frame_portal_stamps.set((a + 1, b));
                                        }
                                        let quad: Vec<[f32; 4]> =
                                            clipped.iter().map(|s| s.to_clip(fw, fh)).collect();
                                        gpu.draw_portal_poly(
                                            per_frame,
                                            &quad,
                                            portal_stamp_mask::INDOOR,
                                        )?;
                                    }
                                }
                            }
                        }
                    }
                    IndoorStep::EnvCellMeshes => {
                        for id in view.draw_order() {
                            let Some((cell, origin)) = placed.get(&id.0) else {
                                continue;
                            };
                            Self::draw_env_cell(
                                gpu,
                                per_frame,
                                cell,
                                *origin,
                                cell_set.as_deref(),
                                env_detail,
                            )?;
                        }
                    }
                    IndoorStep::Objects => {
                        for id in view.draw_order() {
                            let Some((cell, origin)) = placed.get(&id.0) else {
                                continue;
                            };
                            self.draw_cell_statics(gpu, per_frame, &cell.statics, *origin)?;
                        }
                    }
                    // Flush the alpha list at depth 0.0: the queue everything that blends was
                    // put on. The interior statics have no deferred queue (a standing deviation), so
                    // the blending batches are simply drawn last — which for one traversal is the
                    // same order.
                    IndoorStep::FlushAlphaList => {
                        for id in view.draw_order() {
                            let Some((cell, origin)) = placed.get(&id.0) else {
                                continue;
                            };
                            self.draw_cell_statics(gpu, per_frame, &cell.statics_blended, *origin)?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        }

        /// One interior cell's baked objects,
        /// reduced to what this build has.
        ///
        /// The client draws a cell's objects as shadow parts, sorted by viewer distance and
        /// re-selected for LOD every frame during cell updating. This
        /// build bakes them into per-surface batches exactly as it does outdoors, so what is
        /// left here is the block push and the draw.
        ///
        /// * **Level selection.** An interior cell has the same multi-level batch a block
        ///   gets — every level of every placement in one vertex pool, a `Vec<LevelChunk>` of
        ///   `(placement, level, byte range)` over it, and `WorldScene::refresh_degrade_levels`
        ///   re-assembling `cell.statics` and `cell.statics_blended` from `cell.degrade` on the
        ///   frames a level changes. So a cell's furniture **does** degrade with distance.
        /// * **`objects::parts::select_level` has no caller on *this* path**,
        ///   and that is correct rather than a gap: a baked batch has no moving part to hand
        ///   it, so [`select_levels`] asks `objects::degrade::get_degrade` and the draw-frame
        ///   calculation directly. `select_level`'s production caller is
        ///   `WorldScene::refresh_part_levels`, for everything that moves.
        ///
        /// The one consequence that is real is stated rather than hidden: **no per-object
        /// depth sort**, which is visible only where two *translucent* objects overlap in one
        /// cell. The parts of everything that *moves* get a viewer-distance sort;
        /// a baked batch has no part to sort.
        fn draw_cell_statics(
            &self,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
            batches: &[StaticBatch],
            origin: (f32, f32),
        ) -> Result<(), RenderError> {
            if batches.is_empty() || !self.cfg.cell_statics {
                return Ok(());
            }
            let world = world_constants(&Frame::new(
                Vec3::new(origin.0, origin.1, 0.0),
                Quat::IDENTITY,
            ));
            for b in batches {
                // The per-cell object draw draws these as parts, each through
                // the inner mesh draw's `minimize_object_lighting` with its own drawing
                // sphere; a baked batch offers the union sphere of the statics it merged.
                let set = self.cfg.object_lighting.then(|| {
                    let c = Vec3::new(
                        b.sphere.0.x + origin.0,
                        b.sphere.0.y + origin.1,
                        b.sphere.0.z,
                    );
                    self.object_light_set(c, b.sphere.1, false)
                });
                submit_static_batch(
                    gpu,
                    per_frame,
                    &world,
                    b,
                    set.as_deref(),
                    self.current_detail(dereth_world_render::detail::DetailClass::Building),
                )?;
            }
            // "Multiple Pass Alpha": the cell's clip-mapped batches again, blended, with surface
            // setup's force-alpha argument. This path draws every batch in place rather than
            // queueing any, so the second pass follows the cell's own statics directly instead of
            // waiting for a flush.
            if self.cfg.render.multi_pass_alpha {
                let detail =
                    self.current_detail(dereth_world_render::detail::DetailClass::Building);
                for b in batches
                    .iter()
                    .filter(|b| static_multipass_member(b, detail.is_some()))
                {
                    let set = self.cfg.object_lighting.then(|| {
                        let c = Vec3::new(
                            b.sphere.0.x + origin.0,
                            b.sphere.0.y + origin.1,
                            b.sphere.0.z,
                        );
                        self.object_light_set(c, b.sphere.1, false)
                    });
                    submit_static_batch_with(
                        gpu,
                        per_frame,
                        &world,
                        b,
                        set.as_deref(),
                        detail,
                        true,
                    )?;
                }
            }
            Ok(())
        }

        /// Every resident block's environment cells as the traversal wants them, with
        /// the two viewpoint facts computed per portal in that cell's
        /// own space.
        ///
        /// A cell reached through a portal from another block is addressed by its full id, so the
        /// map spans blocks exactly as the client's visible-cell table does. The second map is the draw-side
        /// lookup — the cell's baked meshes and its block's origin — because the *order* comes from
        /// the traversal and not from the map.
        ///
        /// Both the indoor path ([`Self::draw_inside`]) and the outdoor building path
        /// ([`Self::draw_building_interiors`]) need this, which is why it is not inside either.
        fn traversal_cells(&self, ws: &WorldState) -> (TraversalCells, PlacedCells<'_>) {
            use dereth_world_render::cells::portal_view::{
                CellPortal as ViewPortal, TraversalCell,
            };
            let mut cells: BTreeMap<u32, TraversalCell> = BTreeMap::new();
            // ORDER-OK: a lookup for the draw, keyed by cell id; the draw order is the traversal's.
            let mut placed: BTreeMap<u32, (&EnvCellDraw, (f32, f32))> = BTreeMap::new();
            for block in self.blocks.values() {
                for c in &block.env_cells {
                    let vp = Vec3::new(
                        ws.camera.position.x - block.origin.0,
                        ws.camera.position.y - block.origin.1,
                        ws.camera.position.z,
                    );
                    let local = dereth_physics::math::globaltolocal(&c.frame, vp);
                    let portals = c
                        .portals
                        .iter()
                        .map(|p| {
                            // Cell initialisation folds the largest squared distance from the viewpoint to
                            // any of the portal polygon's vertices into `max_indist`. The seam type
                            // carries four slots because a portal polygon is a quad in the shipped
                            // data; filling all four with the maximum gives the same `max_indist`
                            // for the handful that are not.
                            let mut far = 0.0f32;
                            for v in &p.vertices {
                                let (dx, dy, dz) = (local.x - v.x, local.y - v.y, local.z - v.z);
                                far = far.max(dx.mul_add(dx, dy.mul_add(dy, dz * dz)));
                            }
                            ViewPortal {
                                portal_side: p.portal_side,
                                other_cell_id: p.other_cell_id,
                                other_portal_id: p.other_portal_id,
                                exact_match: p.exact_match,
                                vertex_dist_sq: [far; 4],
                                // `d` is the portal plane's normal dotted with the viewpoint, plus its `d`, in cell space.
                                viewpoint_side_distance: p.plane_normal.x.mul_add(
                                    local.x,
                                    p.plane_normal.y.mul_add(
                                        local.y,
                                        p.plane_normal.z.mul_add(local.z, p.plane_d),
                                    ),
                                ),
                            }
                        })
                        .collect();
                    cells.insert(c.id.0, TraversalCell { id: c.id, portals });
                    placed.insert(c.id.0, (c, block.origin));
                }
            }
            (cells, placed)
        }

        /// Draw one interior cell's mesh, in its block's
        /// frame. The cell's own placement frame is already folded into the vertices at bake.
        fn draw_env_cell(
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
            cell: &EnvCellDraw,
            origin: (f32, f32),
            lights: Option<&[D3dLight]>,
            // The environment detail surface and tiling use DESTCOLOR with
            // INVSRCALPHA; they are installed before the mesh is submitted and cleared afterward.
            // `None` is the null-surface case.
            detail: Option<(TextureSlot, f32)>,
        ) -> Result<(), RenderError> {
            let mut world = world_constants(&Frame::new(
                Vec3::new(origin.0, origin.1, 0.0),
                Quat::IDENTITY,
            ));
            // The subset draw's burned branch: the emissive comes
            // from the vertex (the emissive colour source is set to the vertex), and Diffuse and Ambient
            // from the default material, which it sets to 1.0 in every channel and selects with
            // the from-material source -- so `material_lighting` binds a
            // material of diffuse 1, emissive 0, and `lights` is `minimize_envcell_lighting`'s
            // dynamic set.
            if let Some(l) = lights {
                bind_lights(&mut world, l, 0.0, true);
                world.material_lighting = [0.0, 1.0, 1.0, 0.0];
            }
            for m in &cell.meshes {
                // The environment-cell draw marks this as building geometry.
                // The building-texture gate defaults to 1: skip nontextured subsets, including opaque portal
                // fillers. Keep their geometry for traversal/physics and ordinary-object draws.
                if !dereth_world_render::objects::draw::should_draw_mesh_subset(
                    m.surface_type,
                    true,
                    true,
                    false,
                ) {
                    continue;
                }
                if let Some(slot) = m.texture {
                    gpu.bind_texture(slot, m.sampler);
                }
                // The subset draw's use-detail flag: a detail surface is installed, the
                // device can do it in one pass, and the mesh has a tiling factor. The last is
                // always true here: mesh construction builds every graphics-object and environment-cell
                // mesh with one (1.0 and 3.0), and
                // re-tiles to the current detail tiling on the first draw that disagrees.
                let mut world = world;
                if let Some((slot, tiling)) = detail {
                    gpu.bind_stage1_texture(slot);
                    world.detail_params = [tiling, 1.0, 0.0, 0.0];
                }
                gpu.draw_dynamic(
                    if detail.is_some() {
                        &m.key_detail
                    } else {
                        &m.key
                    },
                    &DrawConstants {
                        alpha_ref: m.alpha_ref,
                        ..DrawConstants::default()
                    },
                    per_frame,
                    &world,
                    &m.vertices,
                )?;
            }
            Ok(())
        }

        /// Flush the alpha list at 0.0, drawing everything it has accumulated.
        ///
        /// Building drawing issues this as its **first** line, before the
        /// portal pass and before the shell. It is not tidying: it is the statement that everything
        /// translucent queued so far belongs *behind* this building.
        ///
        /// **What "the alpha list" is here.** This build has no per-frame queue for statics: the
        /// scene bakes each landblock's statics into per-surface batches at stream time and buckets them
        /// per block, so `BlockDraw::blended` is a *static* bucket rather than a queue being filled
        /// as the frame runs. The flush is therefore "every alpha-list batch of every block already
        /// drawn", and `alpha_pending` is what stands in for the queue's contents. That is the same
        /// set the client's list would hold at the same point, because blocks are walked outermost
        /// ring first and a block's own translucency is queued while that block is drawn.
        ///
        /// **What is *not* flushed**, and this is the part that matters: the client's
        /// alpha-list append queues a *translucent* surface. A clip-mapped
        /// (alpha-tested) one is drawn immediately, in place, with the depth write **on**. Both land
        /// in `BlockDraw::blended` here, because that bucket is keyed on `alpha_blend` alone — so
        /// flushing the whole bucket would move Holtburg's foliage out of the landscape pass and
        /// change the frame everywhere. [`static_alpha_list_member`] also honours the mesh draw's
        /// pre-queue building guard; the alpha-tested remainder stays in the landscape pass, a
        /// standing deviation.
        fn flush_alpha_list(
            &self,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
            pending: &[(i32, i32)],
            flushed: &mut std::collections::HashSet<(i32, i32)>,
            // Which of the two flush points this is -- a building's own or the frame's. It
            // changes nothing about the draws; it decides which counter they land in.
            when: AlphaFlush,
        ) -> Result<(), RenderError> {
            // The outdoor pass's own set (the sun), which is what the slots hold when
            // building drawing flushes.
            let sun_set = self
                .cfg
                .object_lighting
                .then(|| self.object_light_set(Vec3::ZERO, 0.0, true));
            let detail = self.current_detail(dereth_world_render::detail::DetailClass::Building);
            // A building's flush is a whole flush of its own: the clip list first, which here is
            // the second passes "Multiple Pass Alpha" queued in the cells walked so far.
            if when == AlphaFlush::Building {
                self.frame_alpha_order
                    .borrow_mut()
                    .push(AlphaDraw::FlushStart);
                self.drain_multipass_pending(gpu, per_frame)?;
            }
            // A clip-mapped subset the option queued is on the clip list, not this one, even when
            // its surface blends (`Translucent | ClipMap`): it was drawn inside the walk and its
            // second pass went out above.
            let blended = |b: &&StaticBatch| {
                static_alpha_list_member(b)
                    && !(self.cfg.render.multi_pass_alpha
                        && static_multipass_member(b, detail.is_some()))
            };
            let mut stats = self.frame_landscape_alpha.get();
            for key in pending {
                let Some(block) = self.blocks.get(key) else {
                    continue;
                };
                if !block.blended.iter().any(|b| blended(&b)) {
                    continue;
                }
                let world = world_constants(&Frame::new(
                    Vec3::new(block.origin.0, block.origin.1, 0.0),
                    Quat::IDENTITY,
                ));
                for batch in block.blended.iter().filter(blended) {
                    submit_static_batch(gpu, per_frame, &world, batch, sun_set.as_deref(), detail)?;
                    match when {
                        AlphaFlush::Building => stats.blend_early += 1,
                        AlphaFlush::Frame => stats.blend += 1,
                    }
                    self.frame_alpha_order
                        .borrow_mut()
                        .push(AlphaDraw::StaticBlend);
                }
                flushed.insert(*key);
            }
            self.frame_landscape_alpha.set(stats);
            Ok(())
        }

        /// The landscape's clip-list entries under "Multiple Pass Alpha": every clip-mapped
        /// batch of the blocks [`Self::frame_multipass_pending`] holds, drawn again with surface
        /// setup's force-alpha argument, in the order the walk drew their first passes. Drains the
        /// queue, so the next flush starts from what the walk queues after this one.
        ///
        /// That state blends `SRCALPHA / INVSRCALPHA`, drops the alpha test and writes no depth,
        /// and keeps the `LESS` depth test. The batch's first pass wrote depth wherever its
        /// texels passed the alpha test, so this pass fails there and lands only on the texels
        /// the test cut away, blending the soft edge of a leaf over whatever is behind it.
        fn drain_multipass_pending(
            &self,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
        ) -> Result<(), RenderError> {
            let blocks = std::mem::take(&mut *self.frame_multipass_pending.borrow_mut());
            if blocks.is_empty() {
                return Ok(());
            }
            let sun_set = self
                .cfg
                .object_lighting
                .then(|| self.object_light_set(Vec3::ZERO, 0.0, true));
            let detail = self.current_detail(dereth_world_render::detail::DetailClass::Building);
            let mut stats = self.frame_landscape_alpha.get();
            for key in &blocks {
                let Some(block) = self.blocks.get(key) else {
                    continue;
                };
                let world = world_constants(&Frame::new(
                    Vec3::new(block.origin.0, block.origin.1, 0.0),
                    Quat::IDENTITY,
                ));
                for batch in block
                    .blended
                    .iter()
                    .filter(|b| static_multipass_member(b, detail.is_some()))
                {
                    submit_static_batch_with(
                        gpu,
                        per_frame,
                        &world,
                        batch,
                        sun_set.as_deref(),
                        detail,
                        true,
                    )?;
                    stats.multipass += 1;
                    self.frame_alpha_order
                        .borrow_mut()
                        .push(AlphaDraw::StaticForced);
                }
            }
            self.frame_landscape_alpha.set(stats);
            Ok(())
        }

        /// The frame's own alpha flush, at the point the client issues it: after the pass that
        /// queued the meshes **and after the objects that pass drew**.
        ///
        /// Drains [`Self::frame_alpha_pending`], so a second call on the same frame draws nothing.
        fn flush_pending_alpha_list(
            &self,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
        ) -> Result<(), RenderError> {
            // The clip list drains first. The object pass has already drawn these second passes
            // between its own clip and blend entries; this finds the queue empty unless no object
            // pass ran.
            self.drain_multipass_pending(gpu, per_frame)?;
            let pending = std::mem::take(&mut *self.frame_alpha_pending.borrow_mut());
            if pending.is_empty() {
                return Ok(());
            }
            let mut flushed = std::collections::HashSet::new();
            self.flush_alpha_list(gpu, per_frame, &pending, &mut flushed, AlphaFlush::Frame)
        }

        /// Draw the **outdoor** half of the portal
        /// machinery, for every building of one block.
        ///
        /// The building's portals become the outdoor view's portal list; the viewer distance of the
        /// first part is updated; the alpha list is flushed at 0.0; the first part is drawn
        /// portals-only (the BSP's portal pass 1, then 2); and then it is drawn again as the shell.
        ///
        /// The decisions are all the world-render crate's ([`dereth_world_render::cells::portal_view::draw_portal`],
        /// [`dereth_world_render::cells::clip`] and
        /// [`dereth_world_render::objects::buildings::portal_pass`]); what is here is the per-frame
        /// input — the viewpoint in the building's own space, its portal polygons' planes, and the
        /// projection that turns a portal polygon into a screen outline — and the draws.
        ///
        /// **Performed:**
        ///
        /// * The `mode = 1` **depth stamp** is drawn: `Gpu::draw_portal_poly` receives the
        ///   surface-type value and mask 7,
        ///   whose bit 0 stamps the constant `0.999999`. It resets the depth inside each visible
        ///   opening to the far plane, undoing what the landscape already wrote behind the
        ///   building — which is the outdoor half's counterpart of the indoor path's `Clear(4)`.
        /// * Portal clipping, `poly_clip_finish` and view copying are **performed**, not stubbed: an opening
        ///   is clipped to the viewport, and each cell the traversal reaches is clipped to what is
        ///   left of the opening it was reached through.
        ///
        /// **What is still not performed, named rather than skipped silently.**
        ///
        /// * The viewer-distance update of the shell itself, here: the shell's degrade level is
        ///   chosen once per frame with every other baked placement's
        ///   ([`Self::refresh_degrade_levels`]), not in this traversal.
        /// * The per-object test against the view polygons. The traversal is
        ///   clipped at **cell** granularity here; the client also culls each object in a cell
        ///   against every one of that cell's view polygons. A cell that survives the clip
        ///   therefore draws all of its objects, where the client would drop the ones outside the
        ///   opening.
        /// * The other-portal clip, the second clip a portal whose two sides do not coincide
        ///   (`exact_match == 0`) gets against the far cell's own polygon.
        #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
        fn draw_building_interiors(
            &self,
            ws: &WorldState,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
            block: &BlockDraw,
            draw_cell: u16,
            side_cell_count: u8,
            cells: &TraversalCells,
            placed: &PlacedCells<'_>,
            drawn: &mut std::collections::HashSet<u32>,
        ) -> Result<(), RenderError> {
            use dereth_render::pso::portal_stamp_mask;
            use dereth_world_render::cells::clip::{copy_view, get_clip, ScreenPoint, ViewPoly};
            use dereth_world_render::cells::portal_view::{
                build_draw_portals_only, construct_view_clipped, construct_view_from, draw_portal,
                sidedness, Sidedness,
            };
            use dereth_world_render::objects::buildings::portal_pass;

            let (vw, vh) = gpu.size();
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent
            let (fw, fh) = (vw as f32, vh as f32);
            let view = self.view_params(ws, vw, vh);
            let view_proj = dereth_render::camera::projection(&view) * view.view;
            // The one
            // full-screen view polygon the outdoor pass clips its openings against, with its
            // four eye planes; nothing on **this** path reads them (the objects
            // this pass draws are a building's own cells), and the object cull that does is the
            // phase-3 loop in `draw`.
            let eye = self.eye_transform(ws, &view);
            let full = ViewPoly::full_screen(fw, fh, &eye);
            // A building sets sunlight use to 0 before drawing its cells, so they
            // take `minimize_envcell_lighting`'s dynamic set.
            let cell_set = self.cfg.object_lighting.then(|| self.envcell_light_set());
            // Environment-cell drawing installs the environment detail surface before every
            // cell mesh it draws.
            let env_detail =
                self.current_detail(dereth_world_render::detail::DetailClass::Environment);

            // Everything a block draws is issued in that block's frame, so the
            // viewpoint reaches a building already block-local.
            let block_local = Vec3::new(
                ws.camera.position.x - block.origin.0,
                ws.camera.position.y - block.origin.1,
                ws.camera.position.z,
            );
            let block_world = swap_zup_to_d3d()
                * frame_matrix(&Frame::new(
                    Vec3::new(block.origin.0, block.origin.1, 0.0),
                    Quat::IDENTITY,
                ));

            // Project a cell-portal polygon with the transformed-vertex path: the vertices are in
            // the owning cell's space, the draw is in that cell's *block*'s frame, and a cell
            // reached through a portal may belong to another block than this one.
            let project = |c: CellId, p: usize| -> Vec<ScreenPoint> {
                let Some((cell, origin)) = placed.get(&c.0) else {
                    return Vec::new();
                };
                let Some(portal) = cell.portals.get(p) else {
                    return Vec::new();
                };
                let world = swap_zup_to_d3d()
                    * frame_matrix(&Frame::new(
                        Vec3::new(origin.0, origin.1, 0.0),
                        Quat::IDENTITY,
                    ));
                portal
                    .vertices
                    .iter()
                    .map(|v| {
                        let bl = dereth_physics::math::localtoglobal(&cell.frame, *v);
                        let q = world * glam::Vec4::new(bl.x, bl.y, bl.z, 1.0);
                        let q = view_proj * q;
                        ScreenPoint::from_clip([q.x, q.y, q.z, q.w], fw, fh)
                    })
                    .collect()
            };

            for b in block
                .building_views
                .iter()
                .filter(|b| b.draw_cell(side_cell_count) == draw_cell)
            {
                // Part drawing runs inside the building's own position push, so the BSP's
                // splitting planes and its portal polygons' planes are read against a viewpoint in
                // the *building's* space.
                let viewpoint = dereth_physics::math::globaltolocal(&b.frame, block_local);
                let order = build_draw_portals_only(&b.bsp, viewpoint);
                for (mode, poly) in portal_pass(&order) {
                    let Some(p) = b.portal_polygons.get(&poly.polygon) else {
                        continue;
                    };
                    // `d` is the polygon plane's normal dotted with the viewpoint, plus its `d`.
                    let d = p.plane_normal.x.mul_add(
                        viewpoint.x,
                        p.plane_normal.y.mul_add(
                            viewpoint.y,
                            p.plane_normal.z.mul_add(viewpoint.z, p.plane_d),
                        ),
                    );
                    // `get_clip` on the opening's projected points for its sidedness, then `copy_view` of the
                    // result against the other cell's top `portal_view`: the opening's own outline,
                    // projected, clipped to the screen, simplified. `n == 0` or a `copy_view` that
                    // returns 0 is view construction's "return 0".
                    let sd = sidedness(d);
                    let screen: Vec<ScreenPoint> = p
                        .vertices
                        .iter()
                        .map(|v| {
                            let bl = dereth_physics::math::localtoglobal(&b.frame, *v);
                            let q = block_world * glam::Vec4::new(bl.x, bl.y, bl.z, 1.0);
                            let q = view_proj * q;
                            ScreenPoint::from_clip([q.x, q.y, q.z, q.w], fw, fh)
                        })
                        .collect();
                    let clipped = get_clip(&screen, sd == Sidedness::Negative, &full);
                    // `SceneConfig::portal_clip` off is a standing deviation: the clip
                    // answers "visible" and the view copy always succeeds, so the opening keeps the
                    // whole screen as its view and the traversal walks a superset of the cells.
                    let opened = if self.cfg.portal_clip {
                        copy_view(&clipped, &eye)
                    } else {
                        Some(full.clone())
                    };
                    let step =
                        draw_portal(poly, &b.portals, d, mode, &|_| opened.is_some(), &|id| {
                            cells.contains_key(&id)
                        });

                    // Unless the mode is 2, the portal polygon is drawn, with its flag set when the mode is 1 —
                    // the depth stamp.
                    if step.stamp.is_some()
                        && self.cfg.portal_depth_stamp
                        && !is_dummy_portal(&p.vertices)
                    {
                        let quad: Vec<[f32; 4]> =
                            clipped.iter().map(|s| s.to_clip(fw, fh)).collect();
                        gpu.draw_portal_poly(per_frame, &quad, portal_stamp_mask::BUILDING)?;
                    }

                    let (Some(interior), Some(view0)) = (step.interior, opened) else {
                        continue;
                    };
                    // Adding the stab list's views is what gives the reachable cells a `portal_view`
                    // at all, and adding a view to portals refuses a neighbour that has no views —
                    // so the stab list is the traversal's bound. The entered cell is unioned in
                    // rather than assumed: `copy_view(other's top portal_view, ...)` would be
                    // reading `portal_view[-1]` if it were ever absent, and the library test
                    // asserts over the retail data that it never is.
                    let live: BTreeSet<u32> = b.portals[poly.portal_index]
                        .stab_list
                        .iter()
                        .map(|c| c.0)
                        .chain(std::iter::once(interior.cell.0))
                        .collect();
                    let view = if self.cfg.portal_clip {
                        construct_view_clipped(
                            cells,
                            interior.cell,
                            interior.entered_portal,
                            Some(&live),
                            &view0,
                            &eye,
                            &project,
                        )
                    } else {
                        construct_view_from(
                            cells,
                            interior.cell,
                            interior.entered_portal,
                            Some(&live),
                            &|_, _| true,
                        )
                    };
                    // Finishes with cell drawing's mode 1,
                    // the *same* three interior loops — so the cells this outdoor building walk
                    // reaches carry their own `portal_view` exactly as an indoor walk's do, and
                    // their objects are coned against it at. Published into the one map
                    // so that `frame_drawn_cells`' union and `frame_cell_views` agree: a cell in
                    // the first with no entry in the second would be one whose objects this build
                    // still coned against the screen.
                    {
                        let mut published = self.frame_cell_views.borrow_mut();
                        for (id, polys) in &view.cell_views {
                            // A cell reached through two of a building's openings accumulates the
                            // views of both, exactly as retail's view accumulation does.
                            published
                                .entry(*id)
                                .or_default()
                                .extend(polys.iter().cloned());
                        }
                    }
                    // Cell drawing walks the cell draw list backwards: far to near.
                    let mut order: Vec<u32> = Vec::new();
                    for id in view.draw_order() {
                        // Drawn once: a cell visible through two of a
                        // building's openings is drawn once, not twice.
                        if !drawn.insert(id.0) {
                            continue;
                        }
                        let Some((cell, origin)) = placed.get(&id.0) else {
                            continue;
                        };
                        Self::draw_env_cell(
                            gpu,
                            per_frame,
                            cell,
                            *origin,
                            cell_set.as_deref(),
                            env_detail,
                        )?;
                        order.push(id.0);
                    }
                    // Cell drawing's second loop — the cells' objects, after all their meshes.
                    // This is what puts a table in the room you see through a window.
                    for id in &order {
                        let Some((cell, origin)) = placed.get(id) else {
                            continue;
                        };
                        self.draw_cell_statics(gpu, per_frame, &cell.statics, *origin)?;
                    }
                    for id in &order {
                        let Some((cell, origin)) = placed.get(id) else {
                            continue;
                        };
                        self.draw_cell_statics(gpu, per_frame, &cell.statics_blended, *origin)?;
                    }
                }
            }
            Ok(())
        }

        /// Every building portal opening of the resident blocks, in the renderer's world space:
        /// its centre, and the outward direction a viewer has to be on to see through it.
        ///
        /// The outward direction is the sidedness gate read as geometry: `portal_side == 0` admits
        /// a viewer on the polygon's **positive** side, `portal_side == 1` one on its negative
        /// side. So "stand outside a window" is `centre + outward * n`, and nothing about it is a
        /// guess about where the buildings are.
        #[must_use]
        pub fn building_portal_openings(&self) -> Vec<(Vec3, Vec3)> {
            let mut out = Vec::new();
            for block in self.blocks.values() {
                for b in &block.building_views {
                    for node in &b.bsp.nodes {
                        for &(poly, portal) in &node.in_portals {
                            let (Ok(poly), Ok(portal)) =
                                (usize::try_from(poly), usize::try_from(portal))
                            else {
                                continue;
                            };
                            let (Some(p), Some(bp)) =
                                (b.portal_polygons.get(&poly), b.portals.get(portal))
                            else {
                                continue;
                            };
                            #[allow(clippy::cast_precision_loss)] // a polygon has 3..=8 vertices
                            let n = p.vertices.len() as f32;
                            let mut c = Vec3::ZERO;
                            for v in &p.vertices {
                                c = c.add(*v);
                            }
                            let c = Vec3::new(c.x / n, c.y / n, c.z / n);
                            let sign = if bp.portal_side == 0 { 1.0 } else { -1.0 };
                            let centre = dereth_physics::math::localtoglobal(&b.frame, c);
                            let normal = dereth_physics::math::localtoglobal(
                                &Frame::new(Vec3::ZERO, b.frame.rotation),
                                Vec3::new(
                                    p.plane_normal.x * sign,
                                    p.plane_normal.y * sign,
                                    p.plane_normal.z * sign,
                                ),
                            );
                            out.push((
                                Vec3::new(
                                    centre.x + block.origin.0,
                                    centre.y + block.origin.1,
                                    centre.z,
                                ),
                                normal,
                            ));
                        }
                    }
                }
            }
            out
        }

        /// Every building portal the outdoor pass would draw an interior through this frame,
        /// projected to window pixels, for the differential test and the log line.
        ///
        /// This is deliberately *not* the portal clip: it runs the same
        /// [`draw_portal`](dereth_world_render::cells::portal_view::draw_portal) decision the draw does and
        /// then projects the opening's own polygon with the frame's own matrices — so it answers
        /// "where on the screen is this window", which is the only place an interior is allowed to
        /// appear. A vertex behind the eye is dropped rather than projected through the singularity,
        /// which makes the reported outline a *subset* and therefore a conservative one to assert
        /// against; the test dilates it.
        #[must_use]
        pub fn building_portal_screen_polygons(
            &self,
            ws: &WorldState,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            use dereth_world_render::cells::portal_view::{
                build_draw_portals_only, draw_portal, PortalMode,
            };
            let (cells, _) = self.traversal_cells(ws);
            let view = self.view_params(ws, width, height);
            let view_proj = dereth_render::camera::projection(&view) * view.view;
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent
            let (fw, fh) = (width as f32, height as f32);
            let mut out = Vec::new();
            for block in self.blocks.values() {
                let world = swap_zup_to_d3d()
                    * frame_matrix(&Frame::new(
                        Vec3::new(block.origin.0, block.origin.1, 0.0),
                        Quat::IDENTITY,
                    ));
                for b in &block.building_views {
                    let block_local = Vec3::new(
                        ws.camera.position.x - block.origin.0,
                        ws.camera.position.y - block.origin.1,
                        ws.camera.position.z,
                    );
                    let viewpoint = dereth_physics::math::globaltolocal(&b.frame, block_local);
                    for poly in build_draw_portals_only(&b.bsp, viewpoint) {
                        let Some(p) = b.portal_polygons.get(&poly.polygon) else {
                            continue;
                        };
                        let d = p.plane_normal.x.mul_add(
                            viewpoint.x,
                            p.plane_normal.y.mul_add(
                                viewpoint.y,
                                p.plane_normal.z.mul_add(viewpoint.z, p.plane_d),
                            ),
                        );
                        let step = draw_portal(
                            poly,
                            &b.portals,
                            d,
                            PortalMode::BuildView,
                            &|_| true,
                            &|id| cells.contains_key(&id),
                        );
                        if step.interior.is_none() {
                            continue;
                        }
                        // Clip space first, then the near plane, then the divide. Dropping the
                        // behind-the-eye vertices instead would truncate the outline of an opening
                        // that straddles the eye plane, and a truncated outline is a *smaller*
                        // mask — which would make the confinement assertion below fail for a
                        // reason that has nothing to do with the interior.
                        let clip: Vec<glam::Vec4> = p
                            .vertices
                            .iter()
                            .map(|v| {
                                // The polygon is in the building's space; the draw is in the
                                // block's.
                                let bl = dereth_physics::math::localtoglobal(&b.frame, *v);
                                view_proj * world * glam::Vec4::new(bl.x, bl.y, bl.z, 1.0)
                            })
                            .collect();
                        let clipped = clip_to_near_plane(&clip);
                        if clipped.len() < 3 {
                            continue;
                        }
                        let screen: Vec<(f32, f32)> = clipped
                            .iter()
                            .map(|c| {
                                (
                                    (c.x / c.w).mul_add(0.5, 0.5) * fw,
                                    (c.y / c.w).mul_add(-0.5, 0.5) * fh,
                                )
                            })
                            .collect();
                        out.push(screen);
                    }
                }
            }
            out
        }

        /// Every **cell** portal that leads outdoors and that [`IndoorStep::PortalDepthStamps`]
        /// would stamp this frame, projected to window pixels.
        ///
        /// The indoor sibling of [`Self::building_portal_screen_polygons`], and it exists for the
        /// same reason: these polygons are *where the land is allowed to survive* in an indoor
        /// frame, so a test that says "the land still draws through the windows" has somewhere
        /// definite to look. The walk is the draw's own — `Self::viewer_cell`, the traversal, its
        /// draw order, `other_cell_id == 0xFFFFFFFF`, `is_dummy_portal` — and the projection uses
        /// the frame's own matrices.
        ///
        /// A vertex behind the eye is dropped by [`clip_to_near_plane`] rather than projected
        /// through the singularity, and an opening left with fewer than three vertices is dropped
        /// entirely; both make the reported outline a **subset**, which is the conservative
        /// direction for anything asserted against it.
        ///
        /// [`IndoorStep::PortalDepthStamps`]: dereth_world_render::cells::portal_view::IndoorStep::PortalDepthStamps
        #[must_use]
        pub fn indoor_outdoor_portal_screen_polygons(
            &self,
            ws: &WorldState,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            use dereth_world_render::cells::portal_view::construct_view;
            let Some(start) = ws.viewer_cell() else {
                return Vec::new();
            };
            let (cells, placed) = self.traversal_cells(ws);
            if !cells.contains_key(&start.0) {
                return Vec::new();
            }
            let view = self.view_params(ws, width, height);
            let view_proj = dereth_render::camera::projection(&view) * view.view;
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent
            let (fw, fh) = (width as f32, height as f32);
            let traversal = construct_view(&cells, start, &|_, _| true);
            let mut out = Vec::new();
            for id in traversal.draw_order() {
                let Some((cell, origin)) = placed.get(&id.0) else {
                    continue;
                };
                let world = swap_zup_to_d3d()
                    * frame_matrix(&Frame::new(
                        Vec3::new(origin.0, origin.1, 0.0),
                        Quat::IDENTITY,
                    ));
                for p in &cell.portals {
                    if p.other_cell_id != 0xFFFF_FFFF || is_dummy_portal(&p.vertices) {
                        continue;
                    }
                    let clip: Vec<glam::Vec4> = p
                        .vertices
                        .iter()
                        .map(|v| {
                            let bl = dereth_physics::math::localtoglobal(&cell.frame, *v);
                            view_proj * world * glam::Vec4::new(bl.x, bl.y, bl.z, 1.0)
                        })
                        .collect();
                    let clipped = clip_to_near_plane(&clip);
                    if clipped.len() < 3 {
                        continue;
                    }
                    out.push(
                        clipped
                            .iter()
                            .map(|c| {
                                (
                                    (c.x / c.w).mul_add(0.5, 0.5) * fw,
                                    (c.y / c.w).mul_add(-0.5, 0.5) * fh,
                                )
                            })
                            .collect(),
                    );
                }
            }
            out
        }

        /// The screen-space bounding box of every triangle [`Self::draw_cell_statics`] will issue
        /// this frame, for the cells the viewer's own portal traversal reaches.
        ///
        /// This is the interior sibling of [`Self::building_portal_screen_polygons`] and exists for
        /// the same reason: a differential that says "some pixels changed" proves nothing, and the
        /// claim worth asserting is that the pixels changed **where the furniture is**. One box per
        /// triangle rather than one per object, because a cell's objects are merged into batches by
        /// surface and the object boundary is gone by then; the union of the boxes is a superset of
        /// the drawn pixels and nothing else, which is exactly the mask the assertion wants.
        ///
        /// Boxes are `(x0, y0, x1, y1)` in pixels, y down, and a triangle with any vertex behind
        /// the eye is dropped — it cannot be projected, and dropping it can only make the mask
        /// smaller, so it cannot manufacture a pass.
        #[must_use]
        pub fn cell_static_screen_boxes(
            &self,
            ws: &WorldState,
            width: u32,
            height: u32,
        ) -> Vec<(f32, f32, f32, f32)> {
            use dereth_world_render::cells::portal_view::construct_view;
            let Some(start) = ws.viewer_cell() else {
                return Vec::new();
            };
            let (cells, placed) = self.traversal_cells(ws);
            if !cells.contains_key(&start.0) {
                return Vec::new();
            }
            let view = self.view_params(ws, width, height);
            let view_proj = dereth_render::camera::projection(&view) * view.view;
            #[allow(clippy::cast_precision_loss)] // a back-buffer extent
            let (fw, fh) = (width as f32, height as f32);
            let stride = OBJECT_VERTEX_STRIDE;
            let mut out = Vec::new();
            let constructed = construct_view(&cells, start, &|_, _| true);
            for id in constructed.draw_order() {
                let Some((cell, origin)) = placed.get(&id.0) else {
                    continue;
                };
                let world = swap_zup_to_d3d()
                    * frame_matrix(&Frame::new(
                        Vec3::new(origin.0, origin.1, 0.0),
                        Quat::IDENTITY,
                    ));
                for b in cell.statics.iter().chain(&cell.statics_blended) {
                    // The boxes are of what is on screen, so they come from the
                    // assembled subset and not from every level the bake is holding.
                    for tri in drawn_vertices(b).chunks_exact(stride * 3) {
                        let mut clip = [glam::Vec4::ZERO; 3];
                        for (k, v) in tri.chunks_exact(stride).enumerate() {
                            let f =
                                |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
                            clip[k] = view_proj * world * glam::Vec4::new(f(0), f(4), f(8), 1.0);
                        }
                        if clip.iter().any(|c| c.w <= 1.0e-4) {
                            continue;
                        }
                        let (mut x0, mut y0, mut x1, mut y1) =
                            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                        for c in &clip {
                            let x = (c.x / c.w).mul_add(0.5, 0.5) * fw;
                            let y = (c.y / c.w).mul_add(-0.5, 0.5) * fh;
                            x0 = x0.min(x);
                            y0 = y0.min(y);
                            x1 = x1.max(x);
                            y1 = y1.max(y);
                        }
                        out.push((x0, y0, x1, y1));
                    }
                }
            }
            out
        }

        /// How many cells the portal traversal actually reaches from the
        /// viewer's own cell this frame, and how many of them have any mesh to draw. `(0, 0)` is
        /// `draw_inside` bailing, and `(n, m)` with `m << n` is a traversal
        /// that reaches cells whose geometry never arrived.
        #[must_use]
        pub fn indoor_traversal_counts(&self, ws: &WorldState) -> (usize, usize) {
            use dereth_world_render::cells::portal_view::construct_view;
            let Some(start) = ws.viewer_cell() else {
                return (0, 0);
            };
            let (cells, placed) = self.traversal_cells(ws);
            if !cells.contains_key(&start.0) {
                return (0, 0);
            }
            let view = construct_view(&cells, start, &|_, _| true);
            let order = view.draw_order();
            let meshed = order
                .iter()
                .filter(|id| placed.get(&id.0).is_some_and(|(c, _)| !c.meshes.is_empty()))
                .count();
            (order.len(), meshed)
        }

        /// **The renderer's `outside_view.view_count` for this frame — the one number the indoor
        /// arm keys the whole outdoor pass on.**
        ///
        /// The branch opens by testing that the outside view's `view_count` is non-zero, and draws the
        /// landscape. `outside_view` gets one copied view per portal whose
        /// other-cell id is `0xFFFFFFFF` that the clip
        /// finds visible. So **zero is "this interior sees no outdoors from here"** and is exactly
        /// the state in which [`SceneConfig::outside_view_gate`] changes the frame; non-zero
        /// preserves the observed land visible through the windows.
        ///
        /// `None` is "not an indoor frame at all": the viewer is outdoors, or `draw_inside` would
        /// bail because the traversal cannot start from the viewer's cell. Both are states in which
        /// the gate is not the thing under test, and they are distinguished from `Some(0)` because
        /// an instrument that cannot look must not report absence.
        ///
        /// It is the **same** view construction `draw_inside` makes, so this is a reading of the
        /// frame that was drawn rather than a model of it. A **pixel** differential is
        /// unmeasurable at a body station once the camera is inside
        /// the sealed room: the interior covers the frame there, while this number is the branch
        /// itself and does not depend on framing at all.
        #[must_use]
        /// **It is a reading of the frame rather than a model of it.** The draw's own
        /// traversal is `construct_view_clipped` under [`SceneConfig::portal_clip`], and a
        /// probe that re-ran the view construction unclipped would report "the window is visible" on a
        /// frame that ran no outdoor pass at all. So when the last [`Self::draw`] took the indoor
        /// path, this returns **its** number ([`Self::frame_outside_view_count`]); the
        /// re-derivation below is the fallback for a scene that has not drawn yet, and it is the
        /// unclipped superset, which is honest for "no frame has been drawn".
        pub fn indoor_outside_view_count(&self, ws: &WorldState) -> Option<usize> {
            use dereth_world_render::cells::portal_view::construct_view;
            if let Some(n) = self.frame_outside_view_count.get() {
                return Some(n);
            }
            let start = ws.viewer_cell()?;
            let (cells, _) = self.traversal_cells(ws);
            if !cells.contains_key(&start.0) {
                return None;
            }
            Some(construct_view(&cells, start, &|_, _| true).outside_view_count)
        }

        /// How many interior cells the resident blocks carry, and how many the viewer's own portal
        /// traversal reaches this frame. For the log line and the tests.
        #[must_use]
        pub fn env_cell_counts(&self, ws: &WorldState) -> (usize, usize) {
            let resident = self.blocks.values().map(|b| b.env_cells.len()).sum();
            let inside = ws.viewer_cell().map_or(0, |start| {
                let mut n = 0usize;
                for block in self.blocks.values() {
                    for c in &block.env_cells {
                        if c.id == start {
                            n = c.meshes.len();
                        }
                    }
                }
                n
            });
            (resident, inside)
        }

        /// Every resident interior cell, and whether its room is drawn from the other era's
        /// record of it ([`ObjectLook`]) rather than the world's.
        #[must_use]
        pub fn interior_looks(&self) -> BTreeMap<CellId, bool> {
            self.blocks
                .values()
                .flat_map(|b| &b.env_cells)
                .map(|c| (c.id, c.from_look))
                .collect()
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

    impl AppearanceKey {
        fn new(setup: DataId, od: &dereth_protocol::types::ObjDesc) -> Self {
            Self {
                setup,
                palette: od.palette_id,
                subpalettes: od
                    .subpalettes
                    .iter()
                    .map(|x| (x.sub_id, x.offset, x.num_colors))
                    .collect(),
                textures: od
                    .texture_changes
                    .iter()
                    .map(|x| (x.part_index, x.old_tex_id, x.new_tex_id))
                    .collect(),
                parts: od
                    .anim_part_changes
                    .iter()
                    .map(|x| (x.part_index, x.part_id))
                    .collect(),
            }
        }
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
            }
        }

        /// One physics body at a world frame: compose each part's frame, scale its mesh, and append
        /// its triangles to the right group.
        ///
        /// Scaled frame composition places a part
        /// ([`dereth_world_render::objects::parts::combine_scaled`]); the mesh itself is scaled
        /// separately through `gfxobj_scale`, and conflating the two makes parts drift apart.
        fn add_object(&mut self, obj_id: DataId, frame: &Frame, scale: f32) {
            self.add_object_kind(obj_id, frame, scale, false);
        }

        fn add_object_kind(
            &mut self,
            obj_id: DataId,
            frame: &Frame,
            scale: f32,
            building_pass: bool,
        ) {
            let store = self.store;
            let parts = self
                .cache
                .parts
                .entry(obj_id)
                .or_insert_with(|| resolve_parts(store, obj_id))
                .clone();
            let s = Vec3::new(scale, scale, scale);
            for part in parts {
                // The part draw guard `if (gfxobj[deg_level] != NULL)`.
                // A baked mesh stands for the near band, and at `d = 0` eleven of the retail
                // dat's degrade records select their `FLT_MAX` terminator, whose `gfxobj_id` is 0:
                // draw nothing. See [`dereth_client_runtime::models::draws_at_near_band`].
                if self.part_degrades && !self.cache.draws_at_near_band(store, part.gfxobj) {
                    self.cache.parts_not_drawn += 1;
                    continue;
                }
                let world =
                    dereth_world_render::objects::parts::combine_scaled(frame, &part.frame, s);
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
                let Some(info) = record else {
                    self.append_mesh(
                        part.gfxobj,
                        &world,
                        scale,
                        NO_PLACEMENT,
                        0,
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
                    let c = Vec3::new(sc.x * scale, sc.y * scale, sc.z * scale);
                    world
                        .origin
                        .add(dereth_world_render::math::localtoglobalvec(
                            dereth_world_render::math::l2g(world.rotation),
                            c,
                        ))
                };
                // LINT-OK: a placement index bounded by the block's part count. Not a float.
                #[allow(clippy::cast_possible_truncation)]
                let placement = self.placements.len() as u32;
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
                    scale_z: scale,
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
                        scale,
                        placement,
                        level,
                        billboards,
                        building_pass,
                    );
                }
            }
        }

        /// One graphics object's triangles into the group buffers, tagged with the placement and
        /// level they belong to. Split out of [`Self::add_object`].
        // The final flag carries building drawing's owner through the existing per-level bake.
        #[allow(clippy::too_many_arguments)]
        fn append_mesh(
            &mut self,
            gfxobj: DataId,
            world: &Frame,
            scale: f32,
            placement: u32,
            level: u32,
            local: bool,
            building_pass: bool,
        ) {
            let store = self.store;
            let groups = self
                .cache
                .geometry
                .entry(gfxobj)
                .or_insert_with(|| build_gfxobj(store, gfxobj))
                .clone();
            for g in &groups {
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
                };
                let (buf, chunks) = self.groups.entry(key.clone()).or_insert_with(|| {
                    self.order.push(key);
                    (Vec::new(), Vec::new())
                });
                // LINT-OK: a byte offset into one landblock's vertex buffer. Not a float.
                #[allow(clippy::cast_possible_truncation)]
                let start = buf.len() as u32;
                let rot = dereth_world_render::math::l2g(world.rotation);
                for (i, (p, u, v)) in g.vertices.iter().enumerate() {
                    let scaled = Vec3::new(p.x * scale, p.y * scale, p.z * scale);
                    // A billboarding placement is baked in the part's own frame
                    // and transformed at assembly time by `draw_pos`; everything else is baked in
                    // world space. With mode 1 the two are the same
                    // arithmetic in the same order, so the assembled bytes are identical.
                    let w = if local {
                        scaled
                    } else {
                        dereth_world_render::math::localtoglobal(world, scaled)
                    };
                    buf.extend_from_slice(&w.x.to_le_bytes());
                    buf.extend_from_slice(&w.y.to_le_bytes());
                    buf.extend_from_slice(&w.z.to_le_bytes());
                    // The vertex normal, in the same space as the position: the
                    // placement's rotation for a world-space bake, the part's own for a
                    // billboarding one (which `append_billboarded` rotates at assembly). The
                    // uniform `scale` does not touch it -- `D3DRS_NORMALIZENORMALS = 1`.
                    let n = g.normals.get(i).copied().unwrap_or(Vec3::ZERO);
                    let n = if local {
                        n
                    } else {
                        dereth_world_render::math::localtoglobalvec(rot, n)
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
                let chunks = if chunks.iter().any(|c| c.placement != NO_PLACEMENT) {
                    chunks
                } else {
                    Vec::new()
                };
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
                };
                if batch.key.alpha_blend {
                    blended.push(batch);
                } else {
                    opaque.push(batch);
                }
            }
            Ok((opaque, blended, self.placements))
        }
    }

    /// What one [`ObjectBaker`] produced: the opaque batches, the blending ones, and the
    /// degrading placements that index into both.
    type BakedGroups = (Vec<StaticBatch>, Vec<StaticBatch>, Vec<DegradePlacement>);

    /// The one `degrade_distance` this build hands
    /// for a whole `ParticleManager`: `f32::max` over its emitters, floored at
    /// the native no-record default of 100.0.
    ///
    /// **This is a known, declared deviation.** The client asks per *emitter*: particle
    /// preparation's first act is the should-draw-particles test on the emitter's physics object
    /// and `degrade_distance`, where
    /// `degrade_distance` is what emitter setup stored for that emitter
    /// alone. So an emitter whose own `GfxObjDegradeInfo` cuts off at 40 m keeps emitting,
    /// simulating and drawing out to whatever the widest emitter sharing its manager asks for.
    ///
    /// **The narrower ones are not cut at draw by their LOD terminator at the same distance.**
    /// The two cut-offs are computed from different quantities:
    ///
    /// | | compared against | measured from |
    /// |---|---|---|
    /// | the should-draw-particles test | the second-last degrade level's maximum distance, **raw** | the object's origin |
    /// | the draw ([`crate::particles::prepare`]) | `get_degrade`'s bands, over `(d - 50.0).max(0)` | each **particle**, over its own scale |
    ///
    /// The renderer's degrade-distance bias is **50.0**, so the draw's terminator sits about 50 m *beyond*
    /// the emitter's own cut-off rather than at it. The emitter test is the tighter of the two in
    /// the client and it is the one this build is not making.
    ///
    /// The fix is a `dereth-animation` change — `EmitterContext::should_draw` is one flag for the whole
    /// manager and the emitter's own degrade-distance field (which exists but is neither written nor
    /// read) is where the per-emitter number belongs — so it is not worked
    /// around here. A test pins the deviation and is **expected to go red** when the seam lands.
    fn manager_degrade_distance(
        gfx: &ParticleGeometry,
        m: &dereth_animation::ParticleManager,
    ) -> f32 {
        m.iter()
            .map(|e| gfx.max_degrade_distance(e.info.hw_gfxobj_id))
            .fold(crate::particles::DEFAULT_DEGRADE_DISTANCE, f32::max)
    }

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

    impl EmitterDegrade {
        /// The should-draw-particles test at `own_distance` — what the client answers for this emitter.
        #[must_use]
        pub fn client_draws(&self) -> bool {
            self.cypt <= self.own_distance
        }

        /// The should-draw-particles test at `manager_distance` — what this build answers.
        #[must_use]
        pub fn this_build_draws(&self) -> bool {
            self.cypt <= self.manager_distance
        }

        /// The emitter is past its own cut-off and inside its manager's, so it is running and
        /// drawing where the client would have hidden it: the deviation, per emitter.
        #[must_use]
        pub fn drawing_past_its_own_cutoff(&self) -> bool {
            self.this_build_draws() && !self.client_draws()
        }
    }

    /// The point every detail level, part sort distance and particle cut-off is measured from:
    /// the placed viewer, which is the eye the frame is drawn from. With no body (or before the
    /// body's camera is first placed) the free camera is that eye.
    ///
    /// **Not `ws.camera`.** The scene update runs before the camera sweep, and at that moment
    /// `ws.camera` holds the unswept chase camera the update has just placed, about 4.4 m behind
    /// and 2.5 m above the feet where the eye is 2.75 m behind and 2.3 m above. Measured from
    /// there, everything near the player stood about 1.6 m further away than it is drawn, so at a
    /// Degrade Distance of 0 the furniture beside the player dropped a level the eye's own
    /// distance keeps. The placed viewer is the sweep's result from the frame before: when the
    /// camera is still it is the eye exactly, and a moving camera is one frame of travel behind,
    /// as the landblock window's re-centre already is.
    fn detail_viewer(ws: &WorldState) -> Vec3 {
        dereth_client_runtime::world_stream::viewpoint(&ws.character, &ws.camera)
    }

    /// Select the degrade level for a whole block's baked
    /// placements, then the assembly of what that chose.
    ///
    /// Returns whether any placement changed level, i.e. whether the batches have to be rebuilt.
    /// `viewer` is **block-local**, like the vertices.
    fn select_levels(
        placements: &mut [DegradePlacement],
        outdoors: bool,
        viewer: Vec3,
        share_2dsq: f32,
        globals: &dereth_world_render::objects::degrade::DegradeGlobals,
    ) -> (bool, u32, u32) {
        let mut changed = false;
        let mut switches = 0u32;
        let mut turns = 0u32;
        for p in placements.iter_mut() {
            // The client computes the offset from the viewer to the part, then the distance
            // `|v|` and the heading `v / |v|`. Both ends are in the same block frame, so the offset is
            // a subtraction. The viewer-distance update's own tail is used rather than a
            // hand-rolled magnitude, because this path needs the
            // *heading* as well, and the degenerate `(0,0,1)` branch with it.
            let (cypt, heading) = placement_viewer_distance(p, outdoors, viewer, share_2dsq);
            // `get_degrade` at the viewer distance over the z scale.
            let z = p.scale_z;
            let dist = if z != 0.0 { cypt / z } else { cypt };
            let (level, mode) =
                dereth_world_render::objects::degrade::get_degrade(&p.info, dist, globals);
            // LINT-OK: a level index; the longest shipped record has six. Not a float conversion.
            #[allow(clippy::cast_possible_truncation)]
            let level = level as u32;
            if level != p.level {
                p.level = level;
                changed = true;
                switches += 1;
            }
            // **The mode is kept.** Draw-frame calculation turns
            // `draw_pos.frame` toward the viewer for modes 2-5, which world-space vertices in a
            // `StaticBatch` cannot do. The batches hold *part-local* vertices for exactly the
            // placements a record billboards,
            // and [`assemble_batches`] applies this frame as it copies them.
            //
            // The comparison is what keeps the cost down: a billboard whose draw frame has
            // not moved since the last assembly does not ask for one, so a camera standing still
            // reassembles nothing.
            p.mode = mode;
            if p.billboards {
                let draw =
                    dereth_world_render::objects::degrade::calc_draw_frame(&p.frame, mode, heading);
                if draw != p.draw_pos {
                    p.draw_pos = draw;
                    changed = true;
                    turns += 1;
                }
            }
        }
        (changed, switches, turns)
    }

    /// The cell- and object-level halves of the viewer-distance update for one world object: its
    /// land cell's horizontal distance and heading when that cell is more than fifty units away
    /// horizontally, otherwise the object's own distance and heading when the camera is at or past
    /// its share distance horizontally, and `None` when each part measures itself. A particle
    /// emitter object shares at the particle distance, anything else at the object distance.
    fn object_shared_distance(
        ws: &WorldState,
        o: &dereth_client_runtime::world_state::WorldObject,
        cam: Vec3,
        share: &dereth_world_render::degrade_loop::DegradeLevel,
    ) -> Option<(f32, Vec3)> {
        let outdoors = ws
            .object_draw_cell(o)
            .is_some_and(dereth_physics::landdefs::is_outdoors);
        dereth_world_render::objects::parts::object_viewer_distance(
            o.frame.origin,
            outdoors,
            cam,
            share.share_distance_2dsq(o.sim.state.is_particle_emitter()),
        )
    }

    /// One baked placement's viewer distance and heading: its land cell's, when `outdoors` and
    /// that cell is more than fifty units away horizontally; its object's, when the camera is at or
    /// past the object's share distance horizontally; and otherwise its own, measured to its sort
    /// centre. Both ends are in the same block frame, so the offsets are subtractions.
    fn placement_viewer_distance(
        p: &DegradePlacement,
        outdoors: bool,
        viewer: Vec3,
        share_2dsq: f32,
    ) -> (f32, Vec3) {
        p.object_origin
            .and_then(|o| {
                dereth_world_render::objects::parts::object_viewer_distance(
                    o, outdoors, viewer, share_2dsq,
                )
            })
            .unwrap_or_else(|| {
                dereth_world_render::objects::parts::viewer_distance_and_heading(p.centre, viewer)
            })
    }

    /// For **one part**, returning
    /// both the distance it handed `get_degrade` and the level that came back.
    ///
    /// The distance is returned as well as the level so that
    /// [`WorldScene::part_degrade_probe`] can publish the lookup's *input*: a test that only saw
    /// the output could assert the selection agreed with itself and never that it agreed with the
    /// record. This is [`dereth_world_render::objects::parts::select_level`]'s only implementation
    /// path, and both callers go through it.
    fn part_level(
        pl: &PartLevels,
        part: &dereth_animation::parts::PhysicsPart,
        is_player: bool,
        cam: Vec3,
        shared: Option<(f32, Vec3)>,
        globals: &dereth_world_render::objects::degrade::DegradeGlobals,
    ) -> PartFrameChoice {
        use dereth_world_render::objects::degrade::DegradeMode;
        use dereth_world_render::objects::parts::{part_viewer_distance, select_level, PartDraw};
        let mut pd = PartDraw {
            pos: part.pos,
            draw_pos: part.pos,
            gfxobj_scale: part.gfxobj_scale,
            cypt: 0.0,
            deg_level: 0,
            deg_mode: DegradeMode::None,
            no_draw: part.no_draw(),
        };
        // Past the object's share distance the part is handed the object's distance and heading
        // (`shared`); inside it the part measures its own.
        let (cypt, heading) = part_viewer_distance(&pd, pl.sort_center, cam, shared);
        select_level(
            &mut pd,
            pl.info.as_deref(),
            is_player,
            cypt,
            heading,
            globals,
        );
        // `select_level`'s own distance over the z scale -- re-derived rather than returned, because
        // the function's contract is the level and the frame, and this is the probe's business.
        let z = part.gfxobj_scale.z;
        let dist = if z != 0.0 { cypt / z } else { cypt };
        // LINT-OK: a level index into a record whose longest shipped form has six levels.
        #[allow(clippy::cast_possible_truncation)]
        let level = pd.deg_level as u32;
        // `select_level` fills `draw_pos` -- it is
        // the draw-frame calculation's answer and the last line of the transcription -- and
        // dropping it leaves every creature's foliage and flame billboards facing
        // whichever way the animation frame left them. It is returned, with the mode beside
        // it so a probe can say *why* the frame moved.
        //
        // The fourth field, `cypt`, is the viewer distance the update above
        // computed — it is `select_level`'s first act to store it in `PartDraw::cypt` — and it is
        // the object pass's sort key. A struct rather than a tuple:
        // four positional floats and integers is one transposition away from a
        // silent defect.
        PartFrameChoice {
            cypt,
            heading,
            distance: dist,
            level,
            mode: pd.deg_mode,
            draw_pos: pd.draw_pos,
        }
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

    /// Rebuild every chunked batch's [`StaticBatch::active`] from the levels
    /// [`select_levels`] just chose.
    ///
    /// The order of the source buffer is preserved, so a placement's level-*n* mesh is drawn where
    /// its level-0 mesh was: with every placement at level 0 and no `gfxobj_id == 0` level in the
    /// way, this reproduces [`StaticBatch::vertices`] exactly.
    fn assemble_batches(batches: &mut [StaticBatch], placements: &[DegradePlacement]) {
        for b in batches {
            if b.chunks.is_empty() {
                continue;
            }
            b.active.clear();
            for c in &b.chunks {
                let p = if c.placement == NO_PLACEMENT {
                    None
                } else {
                    placements.get(c.placement as usize)
                };
                let draw = match p {
                    None => c.placement == NO_PLACEMENT,
                    Some(p) => p.level == c.level,
                };
                if !draw {
                    continue;
                }
                let src = &b.vertices[c.start as usize..c.end as usize];
                // A billboarding placement's vertices are baked in the part's own
                // frame, so the copy *is* the transform. Everything else was baked in world space
                // and is copied byte for byte, which is what keeps a non-billboarding batch
                // identical to its bake.
                match p {
                    Some(p) if p.billboards => append_billboarded(&mut b.active, src, &p.draw_pos),
                    _ => b.active.extend_from_slice(src),
                }
            }
            // The reach sphere object-light minimization is handed
            // must be where the geometry is *drawn*. The bake-time sphere is the union of
            // `vertices`, and a billboarding placement's chunk is baked in the part's own frame,
            // so that union sits at the block origin (or spans from the origin to
            // the world-space chunks, tens of metres wide): every light would fail
            // the reach test and a potted plant, vase or brazier would stand in a
            // torch-lit room at ambient alone. The client tests each part's drawing sphere
            // placed by `draw_pos` (the view-cone check); the assembled buffer is that
            // placement applied, so its sphere is the union of the placed parts.
            b.sphere = vertex_sphere(&b.active);
        }
    }

    /// Copy one chunk of part-local vertices into an assembled buffer through
    /// the part's draw-position frame.
    ///
    /// The FVF is [`LAND_VERTEX_STRIDE`] = 24 bytes: `float3 position`, `D3DCOLOR diffuse`,
    /// `float2 uv`. Only the position is transformed; the diffuse word
    /// [`stamp_vertex_alpha`] wrote at bake time and the texture coordinates are copied
    /// untouched, so a billboard keeps its per-surface alpha.
    fn append_billboarded(out: &mut Vec<u8>, src: &[u8], draw: &Frame) {
        let stride = OBJECT_VERTEX_STRIDE;
        let rot = dereth_world_render::math::l2g(draw.rotation);
        for v in src.chunks_exact(stride) {
            let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
            let local = Vec3::new(f(0), f(4), f(8));
            let w = dereth_world_render::math::localtoglobal(draw, local);
            out.extend_from_slice(&w.x.to_le_bytes());
            out.extend_from_slice(&w.y.to_le_bytes());
            out.extend_from_slice(&w.z.to_le_bytes());
            // The normal turns with the billboard, as the D3D world matrix would turn it.
            let n =
                dereth_world_render::math::localtoglobalvec(rot, Vec3::new(f(12), f(16), f(20)));
            out.extend_from_slice(&n.x.to_le_bytes());
            out.extend_from_slice(&n.y.to_le_bytes());
            out.extend_from_slice(&n.z.to_le_bytes());
            out.extend_from_slice(&v[OBJECT_DIFFUSE_OFFSET..stride]);
        }
    }

    /// The bounding sphere of a baked buffer's positions: the centre of the axis
    /// box and the farthest vertex from it. Only the object lighting's reach test reads it.
    fn vertex_sphere(vertices: &[u8]) -> (Vec3, f32) {
        let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        let pts = vertices
            .as_chunks::<OBJECT_VERTEX_STRIDE>()
            .0
            .iter()
            .map(|v| {
                let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
                Vec3::new(f(0), f(4), f(8))
            });
        let mut any = false;
        for p in pts.clone() {
            any = true;
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        if !any {
            return (Vec3::ZERO, 0.0);
        }
        let c = Vec3::new(
            (lo.x + hi.x) * 0.5,
            (lo.y + hi.y) * 0.5,
            (lo.z + hi.z) * 0.5,
        );
        let r = pts.map(|p| p.sub(c).mag2()).fold(0.0f32, f32::max).sqrt();
        (c, r)
    }

    /// What a batch actually submits this frame — the assembled subset when it is chunked, and the
    /// whole baked buffer when it is not.
    fn drawn_vertices(b: &StaticBatch) -> &[u8] {
        if b.chunks.is_empty() {
            &b.vertices
        } else {
            &b.active
        }
    }

    fn read_surface(store: &RetailDatStore, id: DataId) -> Option<dereth_assets::Surface> {
        let bytes = store.read_typed(DbType::Surface, id).ok()?;
        dereth_assets::Surface::decode_payload_in(store.era_of(id), id, &bytes).ok()
    }

    /// What one surface record turns into on the device: a pipeline state, an alpha reference, a
    /// texture and a sampler.
    ///
    /// The pipeline-state selection is the render crate's ([`PipelineKey::state_from_surface`]);
    /// this is the caller's half of it — the three-hop texture lookup, and creating the **1x1
    /// solid-colour texture** an untextured surface is drawn through. Surface setup does not
    /// disable the texture stage for an untextured surface: it binds a one-texel image whose alpha
    /// is `(int)((1 - translucency) * 255)`. The render crate computes that word; creating the
    /// texel is the caller's job.
    ///
    /// Shared by the static object baker and by the character, which is why it is a free function
    /// rather than a method on either.
    fn resolve_surface(
        store: &RetailDatStore,
        textures: &TextureStore<'_>,
        gpu: &mut Gpu,
        key: &GroupKey,
        // The image scale derived from `Render.EnvironmentTextureDetail`.
        image_scale: dereth_render::texture::ImageScale,
        // `(uploaded w, uploaded h, source w, source h)` for the caller's census. `None` when this
        // surface uploaded nothing.
        built: &mut Option<(u32, u32, u32, u32)>,
        // Where a compressed texture's mip chain comes from when it is not ready: `None` builds it
        // here, as the upload always has; `Some` asks the worker and reports the surface pending.
        defer: Option<&mut crate::mip_worker::MipWorker>,
    ) -> Result<Option<ResolvedSurface>, RenderError> {
        let (surface, two_sided, tiled) = (key.surface, key.two_sided, key.tiled);
        let decoded = surface.and_then(|id| read_surface(store, id));
        // The clip-map flag is the **surface record's** `BASE1_CLIPMAP` bit, not anything the `RenderSurface`
        // header carries. Combined-texture creation receives the indexed texture, palette, and
        // clip-map flag separately, and the expansion then
        // reads `(bClipMap && idx <= 7) ? 0x00000000 : palette->ARGB[idx]`
        // during palette expansion to ARGB.
        let clip_map = decoded.as_ref().is_some_and(|s| {
            s.surface_type & dereth_render::surface::surface_type::BASE1_CLIPMAP != 0
        });
        // Object-description application substitutes the `SurfaceTexture`
        // the surface names, and replaces the palette the
        // `PFID_INDEX16` expansion reads with the part array's shift palette. The substituted id is
        // a `0x05` and the surface's own is a `0x08`; `TextureStore::resolve` accepts both entry
        // points, so the only thing that changes here is which id the chain starts at.
        let (shift, palette_failures, palette_missing) = match &key.appearance.palette {
            Some(recipe) => match recipe.build(textures) {
                Some((p, failed)) => (Some(p), failed, false),
                None => (None, 0, true),
            },
            None => (None, 0, false),
        };
        let texture_id = key.appearance.texture.or(surface);
        // Combined-texture creation keys the upload, which is the whole of
        // the combined-texture cache: two surface groups naming the same
        // (palette, texture) pair take **one** shared runtime texture and one descriptor pair. See
        // [`combined_key`] for how the key is derived and why a shift palette gets none.
        let (texture_key, palettised) = combined_key(textures, texture_id, shift.as_ref());
        let slot = match texture_id {
            Some(id) => match textures.texture_data_shifted(id, clip_map, shift.as_ref()) {
                Ok(data) => {
                    // Texture upload creates the device
                    // texture at the current texture scale, not at the source extent; see
                    // [`dereth_render::texture::scale_surface`] for the arithmetic and the one
                    // declared departure (the block-compressed arm).
                    let (src_w, src_h) = (data.width, data.height);
                    let data = dereth_render::texture::scale_surface(&data, image_scale)?;
                    // A compressed texture's mip chain from the worker when it can make it: a key
                    // the device already holds is a cache hit and builds nothing, and an uncached
                    // key has no identity for the worker to remember it by.
                    let prepared = match defer {
                        Some(w)
                            if !texture_key.is_uncached() && !gpu.has_texture_key(texture_key) =>
                        {
                            #[allow(clippy::as_conversions)]
                            // LINT-OK: the image scale's shift count, as its own casts elsewhere.
                            w.prepare((texture_key.raw(), image_scale as u32), &data)
                        }
                        _ => crate::mip_worker::Prepared::NotNeeded,
                    };
                    let data = match prepared {
                        crate::mip_worker::Prepared::Pending => return Ok(None),
                        crate::mip_worker::Prepared::Ready(chain) => (*chain).clone(),
                        crate::mip_worker::Prepared::NotNeeded => data,
                    };
                    *built = Some((data.width, data.height, src_w, src_h));
                    Some(gpu.upload_imgtex_keyed(texture_key, &data)?)
                }
                Err(_) => None,
            },
            None => None,
        };
        let state = decoded.map_or_else(RenderState::default, |s| RenderState {
            r#type: s.surface_type,
            handler: SurfaceHandler::Database,
            color_value: s.color_value.unwrap_or(0),
            translucency: s.translucency,
            luminosity: s.luminosity,
            diffuse: s.diffuse,
        });
        let ctx = SurfaceContext {
            // FVF `0x152`, with the normal, and fixed-function lighting on:
            // the subset draw reads a second flag (0 in retail) and
            // pushes 1 for every mesh subset. The vertex shader permutation this selects is the
            // one that runs the fixed-function lighting; `PerDrawConstants::lighting_params`
            // decides per draw whether it does.
            vertex_format: VertexFormat::XyzNormalDiffuseTex1,
            texture_is_set: slot.is_some(),
            tiled,
            two_sided,
            lighting: true,
            ..SurfaceContext::default()
        };
        let st = PipelineKey::state_from_surface(&state, ctx);
        // The *same* surface setup run with a current material whose `has_alpha` is set, which is
        // the only thing that changes about the device before the mesh
        // goes down: the material binding stores `has_alpha` in the material alpha mode
        // and surface setup reads it. Computed here rather than at draw time so that the override
        // is the render crate's own transcription (`SurfaceContext::material_has_alpha`) and not
        // a second copy written on this side of the seam.
        let st_material_alpha = PipelineKey::state_from_surface(
            &state,
            SurfaceContext {
                material_has_alpha: Some(true),
                ..ctx
            },
        );
        // The subset draw passes its use-detail flag as surface setup's
        // detail-in-stage-1 argument; the detail arm differs only in the stage-op
        // chain. Computed here for the same reason `st_material_alpha` is: the render crate owns
        // the transcription of surface setup, and a second copy on this side of the seam is a second
        // chance to disagree with it.
        let st_detail = PipelineKey::state_from_surface(
            &state,
            SurfaceContext {
                detail_in_stage1: true,
                ..ctx
            },
        );
        // The alpha-list flush draws an entry queued by "Multiple Pass Alpha" through surface
        // setup with its force-alpha argument set; nothing else about the device changes. Computed
        // here for the same reason as the two above.
        let st_force_alpha = PipelineKey::state_from_surface(
            &state,
            SurfaceContext {
                force_alpha: true,
                ..ctx
            },
        );
        let (texture, texture_key, solid_texel) = match (slot, st.solid_color) {
            (Some(s), _) => (Some(s), texture_key, false),
            // **The untextured arm is keyed on the colour word.**
            //
            // Surface setup's no-texture branch sets the solid-colour texture colour to
            // `alpha << 0x18 | color_value & 0xFFFFFF` and
            // then binds **one** device-wide solid-colour texture whose texel it has just
            // rewritten — so retail costs a single texture for every untextured surface in the
            // world. This build bakes the slot into the batch and cannot rewrite a texel per
            // draw, so the declared deviation is one 1x1 texture per **distinct colour word**
            // instead of retail's one: pixel-identical, and strictly fewer than one per
            // surface group.
            //
            // **The colour word gets its own key space.** That `0` is not a `DataId` any texture
            // chain can end at is an argument about a value rather than about a construction, and
            // the same key space has two other producers (the UI's image key and the font atlas)
            // for which no such argument holds at all. `TextureSpace::SolidColor` makes it a
            // property of the type:
            // `argb == 0` is still [`dereth_render::TextureKey::UNCACHED`] — one fully transparent
            // black texel, shared with nobody — and every other colour word is in a space no
            // DataID pair can reach.
            (None, Some(argb)) => {
                let ckey = dereth_render::TextureKey::solid_color(argb);
                let s = gpu.upload_texture_keyed(
                    ckey,
                    &dereth_primitives::TextureData {
                        width: 1,
                        height: 1,
                        format: dereth_primitives::TextureFormat::Bgra8,
                        levels: vec![vec![
                            // BGRA byte order, from the packed ARGB word.
                            (argb & 0xFF) as u8,
                            ((argb >> 8) & 0xFF) as u8,
                            ((argb >> 16) & 0xFF) as u8,
                            ((argb >> 24) & 0xFF) as u8,
                        ]],
                    },
                )?;
                (Some(s), ckey, true)
            }
            (None, None) => (None, dereth_render::TextureKey::UNCACHED, false),
        };
        Ok(Some(ResolvedSurface {
            key: st.key,
            surface_type: state.r#type,
            key_material_alpha: st_material_alpha.key,
            key_detail: st_detail.key,
            key_force_alpha: st_force_alpha.key,
            alpha_ref: st.alpha_ref,
            vertex_alpha: st.vertex_alpha,
            luminosity: state.luminosity,
            // `state.r#type` is the surface record's own `type` word, before rendering
            // adds `GOURAUD` and `STIPPLED` to the current surface flags. Mesh construction reads
            // the record's type rather than those render-state flags, so this is the field and not `ty`.
            subset_mask: dereth_world_render::objects::draw::subset_mask(state.r#type),
            texture,
            texture_key,
            solid_texel,
            // The **effective** clip-map: refuses anything that is
            // not `PFID_P8` or `PFID_INDEX16`, and `bClipMap` reaches nothing but the palette
            // expansion, so on any other format the bit cannot change a pixel and two groups
            // disagreeing about it are not in conflict.
            clip_map: clip_map && palettised,
            // 0 = linear/wrap, 1 = linear/clamp — `Gpu::bind_texture`'s table.
            sampler: u32::from(!tiled),
            palette_failures,
            palette_missing,
        }))
    }

    /// The combined-texture cache key for one surface's texture, or
    /// [`dereth_render::descriptor::UNCACHED`] when the original would not have cached it either.
    ///
    /// The native key boundary decides whether a **dyed**
    /// item can collide with the plain texture it was dyed from.
    /// It begins with both key halves zero
    /// and fills the palette
    /// and indexed-texture ids
    /// only when the palette is a maintained dat object.
    /// A zero pair goes to the uncached custom-texture table
    /// and is freed by reference count.
    ///
    /// **Both halves are zeroed together**, so a
    /// modified palette with no maintained dat identity
    /// gives key `0` rather than a key that would name the undyed texture.
    /// That is why this returns `UNCACHED`
    /// for any surface carrying a shift palette.
    /// It distinguishes
    /// "every player in the same outfit shares one upload"
    /// (which [`PaletteComposition`] already achieves one level up,
    /// in [`BakeCache::surfaces`]) from "a dyed shirt overwrites
    /// the undyed one
    /// for everybody".
    ///
    /// The indexed texture's DID is the **`RenderSurface`** the id chain ends at
    /// ([`TextureStore::resolve`]), not the surface or `SurfaceTexture` the group named: that is
    /// the decoded image owned by the shared runtime texture wrapper. Untextured surfaces are keyed by [`resolve_surface`] itself.
    ///
    /// The clip-map flag is deliberately **not** in the key, exactly as the original leaves it out. Two
    /// Surface records sharing a (palette, texture) pair and disagreeing about `BASE1_CLIPMAP`
    /// therefore share one texture, and whichever resolved first decides how it was expanded.
    /// [`BakeCache::clipmap_key_conflicts`] counts that case rather than letting it be invisible.
    /// Returns the key and whether the texture is palettised, i.e. whether `bClipMap` and a shift
    /// palette can change a pixel of it at all.
    fn combined_key(
        textures: &TextureStore<'_>,
        texture_id: Option<DataId>,
        shift: Option<&ExpandedPalette>,
    ) -> (dereth_render::TextureKey, bool) {
        use dereth_render::TextureKey;
        const UNCACHED: TextureKey = TextureKey::UNCACHED;
        let Some(id) = texture_id else {
            return (UNCACHED, false);
        };
        let Ok((rsid, rs, _)) = textures.resolve(id) else {
            return (UNCACHED, false);
        };
        // A `RenderSurface` id of zero would make this key indistinguishable from
        // the untyped shape `(colour, 0)` of a solid-colour key, and would
        // also make `(0, 0)` reachable as a "cached" key. `TextureSpace` settles the first by
        // construction; this refuses the second, so the uncached sentinel keeps meaning exactly
        // what the combined-texture builder's all-zero key means. `resolve` should never
        // answer with a zero id, which is why this is a guard and not an arm with a counter.
        if rsid.0 == 0 {
            return (UNCACHED, false);
        }
        // Combined-texture creation refuses anything that is not `PFID_P8` or `PFID_INDEX16`,
        // so a shift palette on any other format never reaches the palette at all and the
        // texture's own default is what was expanded -- which is a cacheable dat object.
        let palettised = matches!(
            dereth_render::PixelFormatId::from_raw(rs.format),
            dereth_render::PixelFormatId::P8 | dereth_render::PixelFormatId::Index16
        );
        if shift.is_some() && palettised {
            return (UNCACHED, palettised);
        }
        let key = rs.default_palette_id.map_or_else(
            || dereth_render::combined_texture_key(0, rsid.0),
            |p| dereth_render::combined_texture_key(p.0, rsid.0),
        );
        (TextureKey::world(key), palettised)
    }

    /// The `D3DFVF_DIFFUSE` word for one vertex at surface setup's current alpha.
    ///
    /// The diffuse-alpha path writes `alpha << 0x18 | 0xffffff`, i.e.
    /// packed **ARGB** with the RGB left white. The vertex element is declared
    /// `DXGI_FORMAT_B8G8R8A8_UNORM`, whose bytes in memory are B, G, R, A — which is what a
    /// little-endian store of an `0xAARRGGBB` word produces, so the client's own packing carries
    /// over unchanged.
    #[must_use]
    pub(crate) const fn vertex_diffuse(alpha: u8) -> u32 {
        ((alpha as u32) << 24) | 0x00FF_FFFF
    }

    /// Draw one **animated** part: bind the part's material, then
    /// submit its meshes at its own `draw_pos`.
    ///
    /// The part's material is bound, its surfaces installed, the object matrix set from its
    /// graphics-object scale, and then mesh drawing draws the selected level's graphics object.
    ///
    /// The material binding: with a material present it
    /// sets the diffuse colour source to the material and stores `has_alpha` in
    /// the material alpha mode, which surface setup reads. Both effects are here — the constant
    /// colour through `draw_params.w`, the state through [`PartMesh::key_material_alpha`].
    ///
    /// `honour` is [`SceneConfig::material_translucency`]; `counts` is
    /// `(parts submitted, parts drawn through a material alpha)`.
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    fn draw_part(
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
        s: &PartSubmission<'_>,
        honour: bool,
        defer: Option<u32>,
        pass: &mut PartPass,
        lights: Option<&[D3dLight]>,
        // The live `Render.MultiPassAlpha` preference.
        multi_pass_alpha: bool,
    ) -> Result<(), RenderError> {
        use dereth_world_render::objects::alpha::{AlphaEntry, AlphaList};
        use dereth_world_render::objects::draw::classify_subset_passes;

        let (part, draw_pos, meshes) = (s.part, &s.draw_pos, s.meshes);
        pass.counts.parts += 1;
        if honour && material_texture_factor(part.material).is_some() {
            pass.counts.material += 1;
        }
        // The alpha-list append's first-of-kind flag, which is **per list per mesh**
        // and not per mesh: mesh drawing carries two flags (one for the clip list,
        // one for the blend list) and clears only the one whose branch fired.
        let (mut first_clip, mut first_blend) = (true, true);
        for (n, m) in meshes.iter().enumerate() {
            if let Some(index) = defer {
                // The alpha-delay mask is `0x0E`, so every non-opaque subset is deferred.
                // `Render.MultiPassAlpha`, read live by the polygon renderer,
                // reaches this call.
                //
                // [`dereth_world_render::objects::draw::classify_subset_passes`] answers passes,
                // not just a list, because the list
                // alone is not the answer: mesh drawing's multipass arm **falls through** to
                // the object-matrix calculation and the subset draw, so a clip-mapped subset
                // is deferred *and* drawn in place — two device passes — while the delay-mask arm
                // jumps to the loop tail and is one. At the shipped mask both arms
                // choose `AlphaList::Clip`, so wiring the preference to the list alone would change
                // nothing at all.
                let passes = classify_subset_passes(
                    m.subset_mask,
                    dereth_world_render::consts::S_ALPHA_DELAY_MASK,
                    multi_pass_alpha,
                );
                if let Some(list) = passes.list {
                    let first = match list {
                        AlphaList::Clip => std::mem::replace(&mut first_clip, false),
                        AlphaList::Blend => std::mem::replace(&mut first_blend, false),
                    };
                    // LINT-OK: a subset index within one part's mesh; the widest shipped graphics object
                    // has a two-digit surface count. Not a float conversion.
                    #[allow(clippy::cast_possible_truncation)]
                    let surface_num = n as u32;
                    // LINT-OK: a vertex count of one subset of one part. Not a float conversion.
                    #[allow(clippy::cast_possible_truncation)]
                    let verts = (m.vertices.len() / OBJECT_VERTEX_STRIDE) as u32;
                    // The probe travels with the queue and is appended to the trace at
                    // flush time, so the trace is in device submission order.
                    pass.queued.push(QueuedSubset {
                        submission: index,
                        subset: n,
                        probe: PartSubsetDraw {
                            object: s.object,
                            part: s.index,
                            subset: n,
                            cypt: s.cypt,
                            surface: m.surface,
                            mask: m.subset_mask,
                            list: Some(list),
                            first_of_kind: first,
                            outdoors: s.outdoors,
                            before_depth_clear: s.before_depth_clear,
                            // The flush sets it from the entry's own multipass flag.
                            force_alpha: false,
                        },
                    });
                    // LINT-OK: an index into this frame's own queue, capped at
                    // `2 * ALPHA_LIST_CAP` by `AlphaLists` itself. Not a float conversion.
                    #[allow(clippy::cast_possible_truncation)]
                    let handle = (pass.queued.len() - 1) as u32;
                    pass.lists.push(
                        list,
                        AlphaEntry {
                            // The handle indexes `PartPass::queued`, not an uploaded mesh:
                            // `dereth_client` has no mesh handles at all, because every part is
                            // drawn from a dynamic vertex buffer.
                            mesh: dereth_primitives::MeshHandle(handle),
                            surface_num,
                            // The alpha-list append records the *surface*, and the texture is
                            // rebound from it at flush time; this build reads the texture back off
                            // the `PartMesh` the entry names instead of copying the slot here.
                            texture: None,
                            first_of_kind: first,
                            world_matrix: *draw_pos,
                            // When multi-pass alpha is enabled and mask bit 3 is set, the subset is
                            // added to the alpha list with both multi-pass and clipping enabled: the first arm of
                            // `classify_subset`, and the only one that sets the flag.
                            multipass: passes.multipass,
                            range: 0..verts,
                        },
                    );
                    // Falls through; jumps past. Only the
                    // multipass arm draws as well, and that second pass is the whole preference.
                    if !passes.immediate {
                        continue;
                    }
                }
            }
            // Apply the material and render the mesh subset -- the opaque subsets, and every
            // subset at all when `part_alpha_lists` is clear.
            pass.trace.push(PartSubsetDraw {
                object: s.object,
                part: s.index,
                subset: n,
                cypt: s.cypt,
                surface: m.surface,
                mask: m.subset_mask,
                list: None,
                first_of_kind: false,
                outdoors: s.outdoors,
                before_depth_clear: s.before_depth_clear,
                force_alpha: false,
            });
            pass.counts.immediate += 1;
            submit_part_mesh(gpu, per_frame, part, draw_pos, m, honour, lights, false)?;
        }
        Ok(())
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
    }

    /// The bit that marks an alpha-list entry's `mesh` handle as a particle's (indexing
    /// `PartPass::particle_queued`) rather than a part's (indexing `PartPass::queued`).
    const PARTICLE_ENTRY: u32 = 0x8000_0000;

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

    /// Shared submission for outdoor opaque/blended, indoor ordinary statics, and alpha flush.
    /// The geometry accessor remains independent: picking/statistics and LOD retain every face.
    /// Building mesh drawing skips nontextured subsets before either drawing or alpha enqueue.
    fn submit_static_batch(
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
        world: &PerDrawConstants,
        batch: &StaticBatch,
        lights: Option<&[D3dLight]>,
        // Building drawing installs the building-detail surface
        // (with DESTCOLOR / INVSRCALPHA) around the two
        // part-draw calls of a building's shell and clears it afterward.
        // Every other static draw runs with no current detail surface -- part-cell drawing clears
        // it on entry -- so `None` is the rule and the shell is the exception.
        detail: Option<(TextureSlot, f32)>,
    ) -> Result<(), RenderError> {
        submit_static_batch_with(gpu, per_frame, world, batch, lights, detail, false)
    }

    /// [`submit_static_batch`] with surface setup's force-alpha argument, which only the
    /// alpha-list flush of a "Multiple Pass Alpha" entry sets.
    fn submit_static_batch_with(
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
        world: &PerDrawConstants,
        batch: &StaticBatch,
        lights: Option<&[D3dLight]>,
        detail: Option<(TextureSlot, f32)>,
        force_alpha: bool,
    ) -> Result<(), RenderError> {
        if !static_subset_visible(batch) {
            return Ok(());
        }
        let vertices = drawn_vertices(batch);
        if vertices.is_empty() {
            return Ok(());
        }
        if let Some(slot) = batch.texture {
            gpu.bind_texture(slot, batch.sampler);
        }
        let detail = if batch.building_pass { detail } else { None };
        // A baked static has no material clone: the current material is cleared,
        // Diffuse and Ambient from the (white) vertex, Emissive = the surface's luminosity.
        let mut world = *world;
        if let Some(l) = lights {
            bind_lights(&mut world, l, batch.luminosity, false);
        }
        if let Some((slot, tiling)) = detail {
            gpu.bind_stage1_texture(slot);
            world.detail_params = [tiling, 1.0, 0.0, 0.0];
        }
        gpu.draw_dynamic(
            // A batch drawn with a detail surface installed is never queued for a second pass
            // ([`static_multipass_member`]), so the two never meet.
            if force_alpha {
                &batch.key_force_alpha
            } else if detail.is_some() {
                &batch.key_detail
            } else {
                &batch.key
            },
            &DrawConstants {
                alpha_ref: batch.alpha_ref,
                ..DrawConstants::default()
            },
            per_frame,
            &world,
            vertices,
        )
    }

    fn static_subset_visible(batch: &StaticBatch) -> bool {
        // Same authenticated default =1 as draw_env_cell; no invented UI preference.
        // Cell statics are ordinary objects, not environment-cell geometry, hence is_env_cell=false.
        dereth_world_render::objects::draw::should_draw_mesh_subset(
            batch.surface_type,
            true,
            false,
            batch.building_pass,
        )
    }

    fn static_alpha_list_member(batch: &StaticBatch) -> bool {
        static_subset_visible(batch) && is_alpha_list_member(&batch.key)
    }

    /// The other of the two lists a non-opaque subset can be appended to: the alpha-**tested**
    /// cut-outs (catalogue rows 7-9), which keep their depth write.
    ///
    /// `alpha_blend && alpha_test` and [`is_alpha_list_member`]'s `alpha_blend && !alpha_test`
    /// partition the `BlockDraw::blended` bucket, which is keyed on `alpha_blend` alone.
    fn static_clip_list_member(batch: &StaticBatch) -> bool {
        static_subset_visible(batch) && batch.key.alpha_blend && batch.key.alpha_test
    }

    /// Whether "Multiple Pass Alpha", when on, queues this clip-mapped batch for a second pass
    /// at the alpha flush as well as drawing it in place.
    ///
    /// Two things decide it besides the option: the mesh draw consults the alpha lists at all
    /// (not while a detail surface is installed, which for a static is a building's shell drawn
    /// with the building detail texture), and the subset's mask is the clip-mapped one, 8. An
    /// `Alpha | ClipMap` surface is alpha-tested too but is mask 2, so it has one pass. A
    /// `Translucent | ClipMap` surface is mask 8 and so takes both passes, although surface setup
    /// draws its first one blended rather than alpha-tested.
    fn static_multipass_member(batch: &StaticBatch, detail_installed: bool) -> bool {
        use dereth_world_render::consts::S_ALPHA_DELAY_MASK;
        use dereth_world_render::objects::draw::{
            classify_subset_passes, mesh_draw_defers, subset_mask,
        };
        static_subset_visible(batch)
            && batch.key.alpha_blend
            && mesh_draw_defers(
                false,
                S_ALPHA_DELAY_MASK,
                batch.building_pass && detail_installed,
            )
            && classify_subset_passes(subset_mask(batch.surface_type), S_ALPHA_DELAY_MASK, true)
                .multipass
    }

    /// Which of a subset's precomputed pipeline states one draw of it uses.
    ///
    /// Surface setup takes the force-alpha argument and the current material's alpha flag as
    /// two inputs. With force alpha set the material flag cannot change the answer: the surface
    /// already blends without an alpha test, which is the one case the material override leaves
    /// alone. So force alpha wins, then the material, then the plain state.
    pub(crate) fn part_subset_key(
        m: &PartMesh,
        material_alpha: bool,
        force_alpha: bool,
    ) -> &PipelineKey {
        if force_alpha {
            &m.key_force_alpha
        } else if material_alpha {
            &m.key_material_alpha
        } else {
            &m.key
        }
    }

    /// One subset of one part on the device, rendered with the
    /// object matrix and the current material already chosen.
    ///
    /// Separate from [`draw_part`] so that a subset drawn *in place* and the same
    /// subset drawn later out of the alpha list go down through one function. Two copies would
    /// be two chances to disagree about `key_material_alpha`, and the whole point of
    /// deferring is that the picture is otherwise unchanged.
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    fn submit_part_mesh(
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
        part: &dereth_animation::parts::PhysicsPart,
        draw_pos: &Frame,
        m: &PartMesh,
        honour: bool,
        lights: Option<&[D3dLight]>,
        // Surface setup's force-alpha argument: set only when the alpha-list flush draws an entry
        // "Multiple Pass Alpha" queued.
        force_alpha: bool,
    ) -> Result<(), RenderError> {
        // Part submission uses `draw_pos.frame`, not
        // `pos.frame` -- the two differ exactly when draw-frame calculation billboarded the
        // part, which for `first-login-walk-jump` is 188 of 3,234 selected part-levels. Submitting
        // `pos` would leave every creature's foliage and flame billboards not facing the camera.
        // `refresh_part_levels` is what fills it; with `SceneConfig::part_billboards` clear it is
        // `part.pos`.
        let mut world = world_constants_scaled(draw_pos, part.gfxobj_scale);
        let factor = if honour {
            material_texture_factor(part.material)
        } else {
            None
        };
        if factor.is_some() {
            // Diffuse colour taken from the material: the constant replaces the vertex colour.
            world.draw_params[3] = 1.0;
        }
        // The same material binding's other two channels — the clone's
        // `Emissive` (luminosity) and `Diffuse` — which must reach the shader too.
        if honour {
            world.material_lighting = material_lighting(part.material);
        }
        // The fixed-function lights this part is drawn with, and the Emissive
        // the subset draw resolves: the surface's own luminosity when positive,
        // otherwise the clone's (the simple luminosity setter), otherwise the default material's zero.
        if let Some(l) = lights {
            let clone = if honour {
                material_lighting(part.material)[0]
            } else {
                0.0
            };
            let emissive = if m.luminosity > 0.0 {
                m.luminosity
            } else {
                clone
            };
            bind_lights(&mut world, l, emissive, false);
        }
        if let Some(slot) = m.texture {
            gpu.bind_texture(slot, m.sampler);
        }
        gpu.draw_dynamic(
            part_subset_key(m, factor.is_some(), force_alpha),
            &DrawConstants {
                alpha_ref: m.alpha_ref,
                texture_factor: factor.unwrap_or(0),
                ..DrawConstants::default()
            },
            per_frame,
            &world,
            &m.vertices,
        )
    }

    /// One part of one moving object, queued for submission this frame. It carries the sort
    /// key, and the alpha lists index into it.
    ///
    /// It exists because the client's object pass has a phase between *deciding what draws* and
    /// *drawing it* -- the first block-draw pass sorts the cell's shadow parts before the second draws
    /// them -- and this is where that phase keeps its parts.
    #[derive(Debug, Clone, Copy)]
    struct PartSubmission<'a> {
        /// The server object this part belongs to, or `None` for the **local body**.
        object: Option<ObjectId>,
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
        /// `WorldScene::object_draw_cell` resolves it.
        ///
        /// Cell drawing installs *that* cell's own `portal_view` before drawing its objects, so the
        /// cone this part is tested against is the cell's
        /// and not the screen's. `None` is the local
        /// body in a scene with no cell, or an object whose cell the walk did not name — both fall
        /// back to the full-screen cone, which is the unclipped superset.
        cell: Option<CellId>,
    }

    /// Material translucency setting and its alpha-state check,
    /// as one answer.
    ///
    /// ```text
    /// applying translucency t: Ambient.a = Diffuse.a = Specular.a = Emissive.a = 1 - t
    /// alpha-state check:        has_alpha = !(all four >= 1.0)
    /// ```
    ///
    /// So `has_alpha` is set exactly when `t > 0`, and `1 - t` is the alpha the whole part is drawn
    /// at. `None` means the part has no cloned material — its material pointer is null, so the
    /// renderer selects the vertex diffuse colour, i.e. the
    /// vertex colour the bake wrote is used and nothing here applies.
    ///
    /// The returned word is the `D3DRS_TEXTUREFACTOR` the shader substitutes for the vertex colour
    /// when `PerDraw::draw_params.w` is 1 (`legacy.hlsl`'s `lerp(i.color, g_textureFactor, w)`),
    /// which is this renderer's material-diffuse source. The RGB is the material's
    /// own default `Diffuse` of (1, 1, 1); the simple-diffuse channel has no driver in this build
    /// and is not applied here.
    ///
    /// **This overrides rather than multiplies the per-vertex alpha**, because the switch
    /// is a *source* selection: with a material bound the vertex colour is not consulted at all.
    #[must_use]
    pub(crate) fn material_texture_factor(m: Option<MaterialOverride>) -> Option<u32> {
        let m = m?;
        // The alpha-state check sees all four alphas at `1 - t`, so the flag is off at exactly t == 0.
        if m.translucency == dereth_animation::parts::DEFAULT_TRANSLUCENCY {
            return None;
        }
        // The client hands D3D the float `1 - t` and the fixed-function pipeline quantises it to
        // the 8-bit diffuse alpha. This renderer quantises here instead, through the same
        // float-to-int truncation surface setup uses for `curr_alpha` and the particle path already
        // uses for `start_trans`, so the two runtime-material channels agree byte for byte.
        let alpha = dereth_primitives::num::to_i32((1.0 - m.translucency) * 255.0).clamp(0, 255);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: clamped to 0..=255 on the line above. Not a float conversion.
        Some(((alpha as u32) << 24) | 0x00FF_FFFF)
    }

    /// The bound material's `Emissive.rgb` and `Diffuse.rgb`, as
    /// [`PerDrawConstants::material_lighting`] wants them: `[luminosity, diffuse, 1, 0]` for a
    /// part with a material clone, all-zero (no material, the vertex colour untouched) otherwise.
    ///
    /// The part's lighting setter writes the clone's emissive RGB from luminosity and its diffuse
    /// RGB from the diffuse scalar, then current-material setup hands the clone to D3D as the lit
    /// material. Unlike [`material_texture_factor`]
    /// this is not gated on the translucency: a clone with default translucency and a lit
    /// `SetLighting(0.99, 1.0)` is exactly what the selection blink makes.
    #[must_use]
    pub(crate) fn material_lighting(m: Option<MaterialOverride>) -> [f32; 4] {
        match m {
            Some(m) => [m.luminosity, m.diffuse, 1.0, 0.0],
            None => [0.0; 4],
        }
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

    /// Overwrite the alpha byte of every vertex's diffuse word in an already-built buffer.
    ///
    /// [`ObjectBaker`] appends a group's vertices long before that group's surface has been
    /// resolved — resolution needs the `Gpu`, and the baker does not hold one — so the alpha is
    /// stamped in [`ObjectBaker::finish`] instead of at append time. The value comes from the one
    /// [`resolve_surface`] returned; nothing here recomputes surface setup.
    /// # Panics
    /// If `vertices` is not a whole number of 36-byte vertices — a partial trailing vertex would
    /// otherwise be skipped silently, and a buffer built at the wrong stride would be stamped in
    /// the wrong places with no complaint at all.
    pub(crate) fn stamp_vertex_alpha(vertices: &mut [u8], alpha: u8) {
        let stride = OBJECT_VERTEX_STRIDE;
        assert_eq!(
            vertices.len() % stride,
            0,
            "{} bytes is not a whole number of {stride}-byte vertices",
            vertices.len()
        );
        let mut at = VERTEX_ALPHA_BYTE;
        while at < vertices.len() {
            vertices[at] = alpha;
            at += stride;
        }
    }

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

    /// One object's surface groups turned into drawable [`PartMesh`]es.
    ///
    /// Shared by the body, the server's objects, the sky and the interior cells: each walks a
    /// a graphics object's (or cell structure's) groups, resolves the group's surface through
    /// the render path and emits the same 24-byte `XyzDiffuseTex1` vertices.
    ///
    /// `place` is the frame folded into the vertices. `None` leaves them in **object space**, which
    /// is what a part re-placed every frame by animation wants; a cell mesh, which
    /// never moves inside its block, passes its own frame and is baked flat.
    ///
    /// `overrides` is the part's clone, i.e. the `ObjDesc` half.
    pub(crate) fn build_meshes(
        store: &RetailDatStore,
        cache: &mut BakeCache,
        textures: &TextureStore<'_>,
        gpu: &mut Gpu,
        groups: &[dereth_client_runtime::models::SurfaceGroup],
        overrides: Option<&dereth_animation::parts::SurfaceOverrides>,
        place: Option<&Frame>,
    ) -> Result<Vec<PartMesh>, RenderError> {
        build_meshes_with(store, cache, textures, gpu, groups, overrides, place, None)
    }

    /// [`build_meshes`], asking `defer` for compressed textures' mip chains. A group whose chain is
    /// still being built comes back untextured and counted in [`BakeCache::pending_surfaces`]; the
    /// caller discards the meshes and builds them again later.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_meshes_with(
        store: &RetailDatStore,
        cache: &mut BakeCache,
        textures: &TextureStore<'_>,
        gpu: &mut Gpu,
        groups: &[dereth_client_runtime::models::SurfaceGroup],
        overrides: Option<&dereth_animation::parts::SurfaceOverrides>,
        place: Option<&Frame>,
        mut defer: Option<&mut crate::mip_worker::MipWorker>,
    ) -> Result<Vec<PartMesh>, RenderError> {
        let mut out = Vec::with_capacity(groups.len());
        for g in groups {
            let appearance = cache.appearance_for(store, textures, g.surface, overrides);
            let key = GroupKey {
                surface: g.surface,
                two_sided: g.two_sided,
                tiled: g.tiled,
                appearance,
            };
            let honour = cache.surface_translucency;
            let r = match defer.as_deref_mut() {
                Some(w) => match cache.resolve_with(store, textures, gpu, key, Some(w))? {
                    Some(r) => r,
                    None => continue,
                },
                None => cache.resolve(store, textures, gpu, key)?,
            };
            // Surface setup's `curr_alpha` is per **surface group**, which is exactly
            // what `resolve` is keyed by, so the whole group's vertices take one word.
            let diffuse = vertex_diffuse(if honour { r.vertex_alpha } else { 0xFF });
            let mut vertices = Vec::with_capacity(g.vertices.len() * OBJECT_VERTEX_STRIDE);
            let rot = place.map(|f| dereth_world_render::math::l2g(f.rotation));
            for (i, (pt, u, v)) in g.vertices.iter().enumerate() {
                let w = match place {
                    Some(f) => dereth_world_render::math::localtoglobal(f, *pt),
                    None => *pt,
                };
                vertices.extend_from_slice(&w.x.to_le_bytes());
                vertices.extend_from_slice(&w.y.to_le_bytes());
                vertices.extend_from_slice(&w.z.to_le_bytes());
                // The normal (`D3DFVF_NORMAL`) the vertex copy
                // copies into the `0x152` vertex; rotated with the folded frame exactly as the
                // position is placed by it, so a cell baked flat keeps its normals in the same
                // space as its positions. Missing normals (a `SurfaceGroup` built by a test
                // without them) fall to zero, which the lighting reads as "faces nothing".
                let n = g.normals.get(i).copied().unwrap_or(Vec3::ZERO);
                let n = match rot {
                    Some(m) => dereth_world_render::math::localtoglobalvec(m, n),
                    None => n,
                };
                vertices.extend_from_slice(&n.x.to_le_bytes());
                vertices.extend_from_slice(&n.y.to_le_bytes());
                vertices.extend_from_slice(&n.z.to_le_bytes());
                // White, at surface setup's vertex alpha. The RGB stays `0xFFFFFF` exactly as the
                // client writes it (the vertex copy's `alpha << 0x18 | 0xffffff`): with
                // the diffuse colour source set to the vertex, this white *is* the material the
                // fixed-function lighting multiplies the lights into, and an
                // env-cell mesh has the static-light burn overwrite it with
                // the burned static light each frame the pool changes. The **alpha** is the
                // surface's current alpha, not a literal.
                vertices.extend_from_slice(&diffuse.to_le_bytes());
                vertices.extend_from_slice(&u.to_le_bytes());
                vertices.extend_from_slice(&v.to_le_bytes());
            }
            out.push(PartMesh {
                key: r.key,
                surface_type: r.surface_type,
                key_material_alpha: r.key_material_alpha,
                key_force_alpha: r.key_force_alpha,
                key_detail: r.key_detail,
                alpha_ref: r.alpha_ref,
                texture: r.texture,
                sampler: r.sampler,
                subset_mask: r.subset_mask,
                surface: g.surface,
                luminosity: r.luminosity,
                vertices,
            });
        }
        Ok(out)
    }

    /// The environment-cell membership test, as this build can express
    /// it: a batch is on the alpha list when it blends and is **not** alpha-tested.
    ///
    /// A clip-mapped surface (material rows 7 and 8) has `alpha_blend` *and* `alpha_test` set and keeps its
    /// depth write; it is drawn in place. A translucent one (rows 3-6 and 10-13) has `alpha_blend`
    /// without `alpha_test` and has the depth write off; it is queued. So alpha blending
    /// without alpha testing defines the queue, exactly the set with `z_write == false`.
    #[must_use]
    fn is_alpha_list_member(key: &dereth_render::PipelineKey) -> bool {
        key.alpha_blend && !key.alpha_test
    }

    /// The "dummy portal" test, verbatim: skip a
    /// polygon whose vertices are **all** at `x = 12`, **all** at `x = -12`, **all** at `y = 12` or
    /// **all** at `y = -12`.
    ///
    /// ```text
    /// bDummyXPos = bDummyXNeg = bDummyYPos = bDummyYNeg = true
    /// for each vertex v:
    ///     if v.x != 12.0:  bDummyXPos = false
    ///     if v.x != -12.0: bDummyXNeg = false
    ///     if v.y != 12.0:  bDummyYPos = false
    ///     if v.y != -12.0: bDummyYNeg = false
    /// if bDummyXPos or bDummyXNeg or bDummyYPos or bDummyYNeg: draw nothing
    /// ```
    ///
    /// These are the placeholder polygons the exporter leaves on a cell boundary; stamping one
    /// would clear the depth over a 24-unit square of nothing. The comparison is `!=` against an
    /// exact float, so it is written that way here too — a value near 12 is not a dummy.
    #[must_use]
    fn is_dummy_portal(vertices: &[Vec3]) -> bool {
        if vertices.is_empty() {
            return false;
        }
        vertices.iter().all(|v| v.x == 12.0)
            || vertices.iter().all(|v| v.x == -12.0)
            || vertices.iter().all(|v| v.y == 12.0)
            || vertices.iter().all(|v| v.y == -12.0)
    }

    /// The `PerFrame` block.
    ///
    /// This was a local reconstruction while `from_view` uploaded its matrices transposed; that is
    /// fixed in `dereth_render` now, so it simply forwards.
    #[must_use]
    pub fn per_frame_constants(v: &ViewParams) -> PerFrameConstants {
        PerFrameConstants::from_view(v)
    }

    /// The `PerDraw` block for one world frame, with the Z-up → D3D swap folded in.
    #[must_use]
    pub fn world_constants(f: &Frame) -> PerDrawConstants {
        PerDrawConstants {
            world: hlsl_matrix(swap_zup_to_d3d() * frame_matrix(f)),
            ..PerDrawConstants::identity()
        }
    }

    /// [`world_constants`] with the part's own `gfxobj_scale` applied first.
    ///
    /// Part drawing scales the mesh and places the part with two separate values: `gfxobj_scale`
    /// scales the *geometry*, while the part-array scale has already been folded into `pos` by
    /// frame composition. Conflating them makes parts
    /// drift apart, which is why this is a separate matrix and not a scaled frame origin.
    #[must_use]
    pub fn world_constants_scaled(f: &Frame, scale: Vec3) -> PerDrawConstants {
        let s = glam::Mat4::from_scale(glam::Vec3::new(scale.x, scale.y, scale.z));
        PerDrawConstants {
            world: hlsl_matrix(swap_zup_to_d3d() * frame_matrix(f) * s),
            ..PerDrawConstants::identity()
        }
    }

    /// The axis conversion swaps forward and up: client `(x, y, z)`
    /// reaches D3D as `(x, z, y)`. `dereth_render::camera::view_from_frame` documents that its input
    /// is already in D3D order, so the world matrix is where the swap belongs.
    fn swap_zup_to_d3d() -> glam::Mat4 {
        glam::Mat4::from_cols(
            glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, 1.0, 0.0),
            glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
            glam::Vec4::W,
        )
    }

    /// A [`Frame`] as a glam column-vector matrix: `M v = R v + t`.
    fn frame_matrix(f: &Frame) -> glam::Mat4 {
        let m = dereth_world_render::math::l2g(f.rotation).0;
        // The rotation matrix stores the local X axis in world space in elements 0..3, the local Y
        // axis in 3..6, and the local Z axis in 6..9; `localtoglobalvec` reads them as
        // `m[0]*vx + m[3]*vy + m[6]*vz`, so each triple is a *column* of the column-vector matrix.
        glam::Mat4::from_cols(
            glam::Vec4::new(m[0], m[1], m[2], 0.0),
            glam::Vec4::new(m[3], m[4], m[5], 0.0),
            glam::Vec4::new(m[6], m[7], m[8], 0.0),
            glam::Vec4::new(f.origin.x, f.origin.y, f.origin.z, 1.0),
        )
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
    mod astra_texture_minification {
        use super::*;
        include!("world_texture_minification_tests.rs");
    }

    #[cfg(test)]
    mod astra_building_shell {
        use super::*;
        use crate::world::DEFAULT_LANDBLOCK;
        include!("world_building_shell_visibility_tests.rs");
    }

    #[cfg(test)]
    mod env_surface_visibility {
        use super::*;
        include!("world_env_surface_visibility_tests.rs");
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Evaluate what `legacy.hlsl` computes for `mul(v, M)`, given the sixteen floats uploaded
        /// into the constant buffer: `result[j] = sum_i v[i] * data[j*4 + i]`, because the cbuffer
        /// matrix is column-major so `M[i][j]` lives at `data[j*4 + i]`.
        fn shader_mul(data: &[f32; 16], v: glam::Vec4) -> glam::Vec4 {
            let a = [v.x, v.y, v.z, v.w];
            let mut out = [0.0f32; 4];
            for (j, o) in out.iter_mut().enumerate() {
                for (i, &vi) in a.iter().enumerate() {
                    *o += vi * data[j * 4 + i];
                }
            }
            glam::Vec4::new(out[0], out[1], out[2], out[3])
        }

        // Oracle: `dereth_render/src/shaders/legacy.hlsl` (`mul(float4(i.pos, 1.0), g_world)` and
        // `mul(world, g_viewProj)`) plus HLSL's documented default column-major cbuffer packing --
        // D3DCompile is called without D3DCOMPILE_PACK_MATRIX_ROW_MAJOR. What the shader computes
        // has to agree with what glam computes, and with `to_cols_array` it does not.
        #[test]
        fn the_shader_reads_the_matrix_layout_that_dere_render_uploads() {
            let m = glam::Mat4::from_cols_array(&[
                1.0, 2.0, 3.0, 4.0, //
                5.0, 6.0, 7.0, 8.0, //
                9.0, 10.0, 11.0, 12.0, //
                13.0, 14.0, 15.0, 16.0,
            ]);
            let v = glam::Vec4::new(0.5, -1.5, 2.0, 1.0);
            let want = m * v;
            assert_eq!(shader_mul(&hlsl_matrix(m), v), want);
            assert_ne!(
                shader_mul(&m.to_cols_array(), v),
                want,
                "if these ever agree the workaround can go"
            );
            let i = glam::Mat4::IDENTITY;
            assert_eq!(hlsl_matrix(i), i.to_cols_array());
        }

        // Oracle: `dereth_render::camera`'s own convention -- "a client-space point (x, y, z) reaches
        // [the view matrix] as (x, z, y)", applied by the world matrix. A point on the ground of a
        // block one step north of the viewer must land in front of a camera looking north.
        #[test]
        fn the_world_matrix_swaps_z_up_into_d3d_and_then_the_view_matrix_agrees() {
            let cam = FreeCamera::new(Vec3::new(96.0, 0.0, 50.0), 0.0, 0.0);
            let view = dereth_render::camera::view_from_frame(&cam.frame());
            let block = Frame::new(Vec3::new(0.0, BLOCK_LENGTH, 0.0), Quat::IDENTITY);
            let world = world_constants(&block);
            // A block-local point at the block's south-west corner, 192 m north of the viewer.
            let p = glam::Vec4::new(96.0, 0.0, 50.0, 1.0);
            let w = shader_mul(&world.world, p);
            // After the swap the client's z is D3D's y.
            assert!(
                (w - glam::Vec4::new(96.0, 50.0, 192.0, 1.0)).length() < 1e-3,
                "{w:?}"
            );
            let v = shader_mul(&hlsl_matrix(view), w);
            assert!(
                (v - glam::Vec4::new(0.0, 0.0, 192.0, 1.0)).length() < 1e-3,
                "{v:?}"
            );
        }

        // Oracle: `dereth_world_render::math::l2g`. A rotated frame's
        // matrix must move a point the same way `localtoglobal` does, or every scenery object with
        // a heading is placed wrong.
        #[test]
        fn the_frame_matrix_agrees_with_the_clients_own_localtoglobal() {
            let h = std::f32::consts::FRAC_PI_4;
            let f = Frame::new(
                Vec3::new(3.0, -7.0, 11.0),
                Quat::new(
                    dereth_primitives::num::math::cosf(h),
                    0.0,
                    0.0,
                    dereth_primitives::num::math::sinf(h),
                ),
            );
            for p in [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 2.0, 0.0),
                Vec3::new(0.0, 0.0, 3.0),
                Vec3::new(-4.0, 5.0, -6.0),
            ] {
                let want = dereth_world_render::math::localtoglobal(&f, p);
                let got = frame_matrix(&f) * glam::Vec4::new(p.x, p.y, p.z, 1.0);
                assert!(
                    (got - glam::Vec4::new(want.x, want.y, want.z, 1.0)).length() < 1e-4,
                    "{p:?}: {got:?} != {want:?}"
                );
            }
        }

        // Oracle: the retail region. A landblock id has to unpack to
        // the (blockX, blockY) the whole landscape path is indexed by, and Holtburg is (0xA9, 0xB4).
        #[test]
        fn the_landblock_id_unpacks_the_documented_way() {
            assert_eq!(block_xy(0xA9B4), (0xA9, 0xB4));
            assert_eq!(landblock_did(0xA9B4), DataId(0xA9B4_FFFF));
            assert_eq!(lbi_did(0xA9B4), DataId(0xA9B4_FFFE));
        }

        use dereth_render::surface::surface_type as st;

        /// **An `expect`, never a skip.** A run that cannot open the dats has proved nothing about
        /// them and must not pass by silently skipping the assertions.
        fn retail_store() -> RetailDatStore {
            let dir = dereth_dat::testing::dat_dir();
            dereth_dat::testing::open_store().unwrap_or_else(|| {
                panic!(
                    "no retail dats under {} -- set DERETH_TEST_DAT_DIR",
                    dir.display()
                )
            })
        }

        /// Surface alpha is opaque without the translucent flag, otherwise it is the truncated
        /// complement of translucency scaled to a byte:
        ///
        /// ```text
        /// if (!(type & TRANSLUCENT)) curr_alpha = 0xFF
        /// else                       curr_alpha = trunc((1.0 - translucency) * 255.0)
        /// ```
        fn curr_alpha(surface_type: u32, translucency: f32) -> u8 {
            if surface_type & st::TRANSLUCENT == 0 {
                return 0xFF;
            }
            let v = dereth_primitives::num::to_i32((1.0 - translucency) * 255.0);
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            {
                // LINT-OK: the client keeps the low byte of the truncated result.
                (v as u32 & 0xFF) as u8
            }
        }

        /// Every surface in `client_portal.dat`, split by what the texture-alpha path does to it.
        struct SurfaceCensus {
            total: usize,
            flagged: usize,
            flagged_nonzero: usize,
            flagged_zero: usize,
            /// Distinct non-zero `translucency` values among the flagged surfaces (by bit
            /// pattern, because that is what the dat stores), and how many surfaces carry each.
            values: BTreeMap<u32, usize>,
            /// One representative surface id per distinct non-zero `translucency`.
            reps: BTreeMap<u32, DataId>,
            /// Surfaces with the flag **clear**, whose vertices must not move at all.
            clear: Vec<DataId>,
            /// Surfaces with the flag set at `translucency == 0`, i.e. `curr_alpha == 0xFF`.
            zero: Vec<DataId>,
        }

        fn census(store: &RetailDatStore) -> SurfaceCensus {
            let ids = store.ids_of(DbType::Surface);
            assert!(
                !ids.is_empty(),
                "client_portal.dat holds no 0x08 surface records at all"
            );
            let mut c = SurfaceCensus {
                total: 0,
                flagged: 0,
                flagged_nonzero: 0,
                flagged_zero: 0,
                values: BTreeMap::new(),
                reps: BTreeMap::new(),
                clear: Vec::new(),
                zero: Vec::new(),
            };
            for id in ids {
                let Some(s) = read_surface(store, id) else {
                    continue;
                };
                c.total += 1;
                if s.surface_type & st::TRANSLUCENT == 0 {
                    if c.clear.len() < 24 {
                        c.clear.push(id);
                    }
                    continue;
                }
                c.flagged += 1;
                if s.translucency == 0.0 {
                    c.flagged_zero += 1;
                    if c.zero.len() < 8 {
                        c.zero.push(id);
                    }
                    continue;
                }
                c.flagged_nonzero += 1;
                let bits = s.translucency.to_bits();
                *c.values.entry(bits).or_insert(0) += 1;
                c.reps.entry(bits).or_insert(id);
            }
            c
        }

        /// The denominator this unit is measured against. Printed in full and asserted on both
        /// sides: a corpus with **no** translucent surface would make every other test here
        /// vacuous, and a corpus with nothing but them would mean the byte-identical assertion
        /// covers nothing.
        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn the_dat_carries_translucent_surfaces_at_more_than_one_translucency() {
            let store = retail_store();
            let c = census(&store);
            eprintln!(
                "Surface translucency: {} surfaces decoded; {} carry TRANSLUCENT ({} at a non-zero \
                 translucency, {} at zero); {} distinct non-zero translucency values",
                c.total,
                c.flagged,
                c.flagged_nonzero,
                c.flagged_zero,
                c.values.len()
            );
            for (bits, n) in &c.values {
                let t = f32::from_bits(*bits);
                eprintln!(
                    "    translucency {t} -> curr_alpha {} on {n} surface(s), e.g. {:?}",
                    curr_alpha(st::TRANSLUCENT, t),
                    c.reps[bits]
                );
            }
            assert!(c.total > 1000, "only {} surfaces decoded", c.total);
            assert!(
                c.flagged_nonzero > 0,
                "no surface would change, so nothing here is tested"
            );
            assert!(
                c.values.len() > 2,
                "only {} distinct translucency values -- a linear map and a wrong-but-monotonic \
                 one agree at the ends, so the sweep needs interior values",
                c.values.len()
            );
            assert!(!c.clear.is_empty(), "no flag-clear surface to hold still");
        }

        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn the_other_two_scalars_setsurface_hands_the_vertex_path_are_counted() {
            let store = retail_store();
            let ids = store.ids_of(DbType::Surface);
            let (mut total, mut luminous, mut diffuse_not_one) = (0usize, 0usize, 0usize);
            for id in ids {
                let Some(s) = read_surface(&store, id) else {
                    continue;
                };
                total += 1;
                if s.luminosity > 0.0 {
                    luminous += 1;
                }
                if (s.diffuse - 1.0).abs() > f32::EPSILON {
                    diffuse_not_one += 1;
                }
            }
            eprintln!(
                "Surface material channels: of {total} surfaces, {luminous} carry luminosity > 0 and \
                 {diffuse_not_one} carry diffuse != 1.0; the fixture counts both material channels."
            );
            assert!(total > 1000, "only {total} surfaces decoded");
        }

        fn warp_gpu() -> Gpu {
            let cfg = dereth_render::device::DeviceConfig {
                width: 64,
                height: 64,
                ..dereth_render::device::DeviceConfig::default()
            };
            Gpu::new(None, &cfg).expect("a D3D12 WARP device")
        }

        /// One triangle on one surface — enough to read the diffuse word out of, and it keeps
        /// the sweep to one texture upload per surface (the descriptor heap is finite).
        fn one_triangle(surface: DataId) -> Vec<dereth_client_runtime::models::SurfaceGroup> {
            vec![dereth_client_runtime::models::SurfaceGroup {
                surface: Some(surface),
                two_sided: false,
                tiled: false,
                normals: Vec::new(),
                vertices: vec![
                    (Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0),
                    (Vec3::new(1.0, 0.0, 0.0), 1.0, 0.0),
                    (Vec3::new(0.0, 1.0, 0.0), 0.0, 1.0),
                ],
            }]
        }

        /// The `D3DFVF_DIFFUSE` word of every vertex of a built buffer.
        fn diffuse_words(vertices: &[u8]) -> Vec<u32> {
            assert_eq!(vertices.len() % OBJECT_VERTEX_STRIDE, 0, "partial vertex");
            let o = OBJECT_DIFFUSE_OFFSET;
            vertices
                .chunks(OBJECT_VERTEX_STRIDE)
                .map(|v| u32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]))
                .collect()
        }

        /// Build one surface's triangle through [`build_meshes`] and return its diffuse words.
        fn words_for(
            store: &RetailDatStore,
            gpu: &mut Gpu,
            honour: bool,
            surface: DataId,
        ) -> Vec<u32> {
            let textures = TextureStore::new(store);
            let mut cache = BakeCache {
                surface_translucency: honour,
                ..BakeCache::default()
            };
            let meshes = build_meshes(
                store,
                &mut cache,
                &textures,
                gpu,
                &one_triangle(surface),
                None,
                None,
            )
            .expect("the surface resolves");
            assert_eq!(meshes.len(), 1, "one group in, one mesh out");
            diffuse_words(&meshes[0].vertices)
        }

        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn a_resolved_surface_is_freed_on_its_last_release_and_not_before() {
            let store = retail_store();
            let mut gpu = warp_gpu();
            let textures = TextureStore::new(&store);
            let mut cache = BakeCache::default();
            // A surface every shipped landscape uses, taken from the dat rather than invented.
            let id = store
                .ids_of(dereth_dat::DbType::Surface)
                .into_iter()
                .find(|id| read_surface(&store, *id).is_some_and(|s| s.orig_texture_id.is_some()))
                .expect("client_portal.dat carries a textured surface record");
            let key = GroupKey {
                surface: Some(id),
                two_sided: false,
                tiled: false,
                appearance: cache.appearance_for(&store, &textures, Some(id), None),
            };
            let a = cache
                .resolve(&store, &textures, &mut gpu, key.clone())
                .expect("resolves");
            let slot = a.texture.expect("the surface names a texture");
            assert_eq!(cache.link_count(slot), Some(1), "one resolve, one link");
            let b = cache
                .resolve(&store, &textures, &mut gpu, key.clone())
                .expect("resolves");
            assert_eq!(
                b.texture,
                Some(slot),
                "the second resolve is a cache hit, not an upload"
            );
            assert_eq!(
                cache.link_count(slot),
                Some(2),
                "a hit adds a reference, as retail's combined-texture creation does"
            );
            assert_eq!(
                cache.textures_uploaded, 1,
                "one pair held for two consumers"
            );

            // First half: one consumer leaves and the entry survives.
            assert!(
                !cache.release_group_texture(&mut gpu, slot),
                "freed at one link remaining"
            );
            assert_eq!(cache.link_count(slot), Some(1));
            assert_eq!(cache.textures_uploaded, 1, "still held");
            assert_eq!(
                gpu.texture_table_stats().frees,
                0,
                "the texture-image link was dropped while a consumer still held the surface"
            );

            // Second half: the last consumer leaves and it is freed.
            assert!(
                cache.release_group_texture(&mut gpu, slot),
                "not freed at zero links"
            );
            assert_eq!(cache.link_count(slot), None, "the entry is gone");
            assert_eq!(cache.textures_uploaded, 0);
            assert_eq!(
                gpu.texture_table_stats().frees,
                1,
                "the descriptor pair went back"
            );
            assert_eq!(cache.unowned_releases, 0);

            // And a release of a slot the cache no longer owns is counted, not silent.
            assert!(!cache.release_group_texture(&mut gpu, slot));
            assert_eq!(
                cache.unowned_releases, 1,
                "a double release must be visible"
            );
        }

        /// **The arithmetic, at every distinct translucency the dat actually uses** rather than
        /// only at the two ends: a linear map and a wrong-but-monotonic map agree at 0 and 1.
        ///
        /// Oracle: `surface alpha conversion`'s own expression, in [`curr_alpha`] above, applied to the
        /// `translucency` read out of the dat at run time. Nothing here is a literal.
        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn build_meshes_writes_setsurfaces_curr_alpha_for_every_translucency_in_the_dat() {
            let store = retail_store();
            let c = census(&store);
            let mut gpu = warp_gpu();
            let mut checked = 0usize;
            for (bits, id) in &c.reps {
                let t = f32::from_bits(*bits);
                let s = read_surface(&store, *id).expect("the representative decodes");
                let want =
                    u32::from(curr_alpha(s.surface_type, s.translucency)) << 24 | 0x00FF_FFFF;
                for w in words_for(&store, &mut gpu, true, *id) {
                    assert_eq!(
                        w, want,
                        "{id:?} at translucency {t}: diffuse {w:#010X}, surface alpha conversion says {want:#010X}"
                    );
                }
                checked += 1;
            }
            eprintln!(
                "Surface alpha arithmetic: {checked} distinct translucency values checked against \
                 surface alpha conversion's own expression, over {} translucent surfaces",
                c.flagged_nonzero
            );
            assert_eq!(
                checked,
                c.values.len(),
                "the sweep skipped a translucency value"
            );
            assert!(
                checked > 2,
                "only {checked} values -- the ends alone do not discriminate"
            );
        }

        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn a_surface_without_the_flag_is_byte_identical_to_the_previous_build() {
            let store = retail_store();
            let c = census(&store);
            let mut gpu = warp_gpu();
            let mut clear = 0usize;
            for id in &c.clear {
                for w in words_for(&store, &mut gpu, true, *id) {
                    assert_eq!(w, 0xFFFF_FFFF, "{id:?} has TRANSLUCENT clear and moved");
                }
                clear += 1;
            }
            let mut zeroed = 0usize;
            for id in &c.zero {
                for w in words_for(&store, &mut gpu, true, *id) {
                    assert_eq!(
                        w, 0xFFFF_FFFF,
                        "{id:?} is TRANSLUCENT at translucency 0 and moved"
                    );
                }
                zeroed += 1;
            }
            eprintln!(
                "Opaque surface alpha: {clear} flag-clear and {zeroed} flagged-at-zero surfaces are \
                 fully opaque (of {} clear and {} flagged-at-zero in the dat)",
                c.total - c.flagged,
                c.flagged_zero
            );
            assert!(
                clear > 0,
                "the control covered no flag-clear surface at all"
            );
            // The second bucket is **empty in this dat** -- measured, not assumed: all 261
            // `TRANSLUCENT` surfaces carry a non-zero `translucency`. Asserting the equality
            // rather than a sample count is what keeps that from reading as a passing check on no
            // data; the asserted total is the denominator.
            assert_eq!(
                zeroed,
                c.zero.len(),
                "the flagged-at-zero sample was not walked"
            );
            assert_eq!(
                c.flagged_zero, 0,
                "the dat has grown a TRANSLUCENT surface at translucency 0; {zeroed} sampled"
            );
        }

        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn clearing_the_switch_reproduces_the_previous_build_exactly() {
            let store = retail_store();
            let c = census(&store);
            let mut gpu = warp_gpu();
            let ids: Vec<DataId> = c
                .reps
                .values()
                .copied()
                .chain(c.clear.iter().copied())
                .collect();
            assert!(ids.len() > 3, "nothing to compare");
            for id in &ids {
                for w in words_for(&store, &mut gpu, false, *id) {
                    assert_eq!(w, 0xFFFF_FFFF, "{id:?} moved with the switch off");
                }
            }
        }

        /// The **other** call site. [`ObjectBaker`] appends vertices before it has a `Gpu` to
        /// resolve the surface with, so its alpha is stamped in `finish`; this asserts the two
        /// paths agree byte for byte on the same geometry, which is the only thing that keeps a
        /// static and a part of the same object from drawing at different opacities.
        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn the_object_baker_writes_the_same_alpha_as_build_meshes() {
            let store = retail_store();
            let mut gpu = warp_gpu();

            // A graphics object that actually carries a translucent surface, **found** rather than
            // named: the first `0x01` whose triangulated groups include one.
            let mut chosen: Option<(DataId, Vec<dereth_client_runtime::models::SurfaceGroup>)> =
                None;
            for id in store.ids_of(DbType::GfxObj) {
                let groups = build_gfxobj(&store, id);
                let translucent = groups.iter().any(|g| {
                    g.surface
                        .and_then(|s| read_surface(&store, s))
                        .is_some_and(|s| {
                            s.surface_type & st::TRANSLUCENT != 0 && s.translucency != 0.0
                        })
                });
                if translucent && groups.iter().any(|g| !g.vertices.is_empty()) {
                    chosen = Some((id, groups));
                    break;
                }
            }
            let (gfxobj, groups) = chosen
                .expect("no graphics object in the dat carries a non-zero translucent surface");

            // The baker's arm.
            let mut cache = BakeCache::default();
            let mut baker = ObjectBaker::new(&store, &mut cache, false, false, false);
            baker.add_object(
                gfxobj,
                &Frame::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY),
                1.0,
            );
            let (opaque, blended, degrade) = baker.finish(&mut gpu).expect("the object bakes");
            assert!(
                degrade.is_empty(),
                "part_degrades is off, so no placement may be registered"
            );

            // What `surface alpha conversion` says, per surface, from the dat.
            let mut want: BTreeSet<u32> = BTreeSet::new();
            for g in &groups {
                if g.vertices.is_empty() {
                    continue;
                }
                let s = g.surface.and_then(|s| read_surface(&store, s));
                let a = s.map_or(0xFF, |s| curr_alpha(s.surface_type, s.translucency));
                want.insert(u32::from(a) << 24 | 0x00FF_FFFF);
            }
            let mut got: BTreeSet<u32> = BTreeSet::new();
            let mut vertices = 0usize;
            for b in opaque.iter().chain(blended.iter()) {
                for w in diffuse_words(&b.vertices) {
                    got.insert(w);
                    vertices += 1;
                }
            }
            eprintln!(
                "Baked surface alpha: {gfxobj:?} baked {vertices} vertices; diffuse words {got:#010X?}, \
                 surface alpha conversion says {want:#010X?}"
            );
            assert!(
                vertices > 0,
                "the baker emitted nothing, so it proves nothing"
            );
            assert_eq!(
                got, want,
                "the baker's vertex alpha is not surface alpha conversion's"
            );
            assert!(
                got.iter().any(|w| *w != 0xFFFF_FFFF),
                "the chosen object baked nothing translucent, so the mutation could not be seen"
            );

            // And the free-function path, on the same groups, agrees.
            let textures = TextureStore::new(&store);
            let mut c2 = BakeCache::default();
            let meshes = build_meshes(&store, &mut c2, &textures, &mut gpu, &groups, None, None)
                .expect("the groups resolve");
            let mut through_build: BTreeSet<u32> = BTreeSet::new();
            for m in &meshes {
                if m.vertices.is_empty() {
                    continue;
                }
                through_build.extend(diffuse_words(&m.vertices));
            }
            assert_eq!(
                through_build, got,
                "build_meshes and ObjectBaker disagree on the alpha"
            );
        }

        /// The packing itself, against the polygon draw's literal
        /// `curr_alpha << 0x18 | 0xffffff`, and the stamp's placement inside a 24-byte vertex.
        #[test]
        fn the_diffuse_word_is_packed_argb_and_the_stamp_lands_on_the_alpha_byte() {
            assert_eq!(vertex_diffuse(0xFF), 0xFFFF_FFFF);
            assert_eq!(vertex_diffuse(0x00), 0x00FF_FFFF);
            assert_eq!(vertex_diffuse(0xBF), 0xBFFF_FFFF);
            let mut v = vec![0u8; OBJECT_VERTEX_STRIDE * 2];
            let o = OBJECT_DIFFUSE_OFFSET;
            v[o..o + 4].copy_from_slice(&vertex_diffuse(0xFF).to_le_bytes());
            v[o + OBJECT_VERTEX_STRIDE..o + OBJECT_VERTEX_STRIDE + 4]
                .copy_from_slice(&vertex_diffuse(0xFF).to_le_bytes());
            stamp_vertex_alpha(&mut v, 0x2A);
            assert_eq!(diffuse_words(&v), vec![0x2AFF_FFFF, 0x2AFF_FFFF]);
            assert_eq!(v[VERTEX_ALPHA_BYTE], 0x2A);
        }

        /// Material translucency updates all four alpha components and their combined alpha flag:
        ///
        /// ```text
        /// applying translucency `t`:
        ///     material_alpha = 1.0 - t;
        ///     Ambient.a = Diffuse.a = Specular.a = Emissive.a = material_alpha;
        ///     update the material's alpha-state flag;
        /// alpha-state check:
        ///     has_alpha = !(1.0 <= Ambient.a && 1.0 <= Diffuse.a
        ///                   && 1.0 <= Specular.a && 1.0 <= Emissive.a)
        /// ```
        ///
        /// Written out here rather than referenced, so the assertion below is against the
        /// client's own expression and not against [`material_texture_factor`]'s.
        fn material_alpha_byte(t: f32) -> Option<u8> {
            // The four `D3DMATERIAL9` alphas written by the translucency operation, kept as four values
            // because the alpha-state check tests four -- it is the *conjunction over all four* that
            // makes `has_alpha` clear, which is why a material whose Specular alpha alone were
            // low would still be flagged.
            let (ambient, diffuse, specular, emissive) =
                (1.0f32 - t, 1.0f32 - t, 1.0f32 - t, 1.0f32 - t);
            let has_alpha =
                !(ambient >= 1.0 && diffuse >= 1.0 && specular >= 1.0 && emissive >= 1.0);
            if !has_alpha {
                return None;
            }
            let a = diffuse;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: truncation towards zero of a value in 0..=255.
            Some((a * 255.0).clamp(0.0, 255.0) as u8)
        }

        /// The material alpha is one minus t at every interior value.
        #[test]
        fn the_material_alpha_is_one_minus_t_at_every_interior_value() {
            let mut checked = 0usize;
            let mut interior = 0usize;
            let mut distinct: BTreeSet<u32> = BTreeSet::new();
            for i in 0..=100u32 {
                #[allow(clippy::cast_precision_loss)] // 0..=100
                let t = i as f32 / 100.0;
                let want = material_alpha_byte(t);
                let got = material_texture_factor(Some(MaterialOverride {
                    translucency: t,
                    diffuse: dereth_animation::parts::DEFAULT_DIFFUSE,
                    luminosity: dereth_animation::parts::DEFAULT_LUMINOSITY,
                }));
                match (want, got) {
                    (None, None) => assert_eq!(i, 0, "has_alpha is clear only at t == 0"),
                    (Some(a), Some(word)) => {
                        assert_eq!(
                            word >> 24,
                            u32::from(a),
                            "t = {t}: material translucency writes 1 - t = {}, so the alpha byte \
                             is {a}, not {}",
                            1.0 - t,
                            word >> 24
                        );
                        // The polygon draw's RGB: the material's default `Diffuse` is (1,1,1).
                        assert_eq!(word & 0x00FF_FFFF, 0x00FF_FFFF, "t = {t}: the RGB moved");
                        distinct.insert(word);
                        if i > 0 && i < 100 {
                            interior += 1;
                        }
                    }
                    (w, g) => panic!("t = {t}: oracle {w:?}, got {g:?}"),
                }
                checked += 1;
            }
            eprintln!(
                "Runtime translucency: {checked} translucencies checked against the material translucency \
                 `1 - t`, {interior} of them strictly interior, {} distinct texture-factor words",
                distinct.len()
            );
            assert_eq!(checked, 101);
            assert_eq!(interior, 99, "the sweep collapsed to its endpoints");
            // 0.5 is where `t` and `1 - t` agree; a sweep that only produced two words could not
            // tell a ramp from a step.
            assert!(
                distinct.len() > 90,
                "only {} distinct alphas over the ramp",
                distinct.len()
            );
            // No material at all means the material pointer is null, which
            // the material binding answers with the vertex as colour source: nothing is substituted.
            assert_eq!(material_texture_factor(None), None);
            // A material carrying only a *lighting* override has all four alphas at 1.0, so
            // the alpha-state check leaves `has_alpha` clear and this channel does nothing. Those two
            // scalars are the plain luminosity and diffuse setters and have no driver here.
            assert_eq!(
                material_texture_factor(Some(MaterialOverride {
                    translucency: dereth_animation::parts::DEFAULT_TRANSLUCENCY,
                    diffuse: 0.5,
                    luminosity: 0.25,
                })),
                None,
                "a lighting-only material must not force the alpha path"
            );
        }

        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn the_material_alpha_replaces_the_surfaces_own_vertex_alpha_rather_than_multiplying_it() {
            let store = retail_store();
            let c = census(&store);
            // A surface the dat actually carries at a translucency whose `curr_alpha` is neither
            // 0x00 nor 0xFF, so the two channels are distinguishable at all.
            let (bits, id) = c
                .reps
                .iter()
                .map(|(b, i)| (*b, *i))
                .find(|(b, _)| {
                    let a = curr_alpha(st::TRANSLUCENT, f32::from_bits(*b));
                    a > 0x20 && a < 0xE0
                })
                .expect("no TRANSLUCENT surface at a mid-range curr_alpha in the dat");
            let surface_t = f32::from_bits(bits);
            let vertex = curr_alpha(st::TRANSLUCENT, surface_t);

            let mut gpu = warp_gpu();
            let textures = TextureStore::new(&store);
            let mut cache = BakeCache::default();
            let meshes = build_meshes(
                &store,
                &mut cache,
                &textures,
                &mut gpu,
                &one_triangle(id),
                None,
                None,
            )
            .expect("the group resolves");
            let words: BTreeSet<u32> = meshes
                .iter()
                .flat_map(|m| diffuse_words(&m.vertices))
                .collect();
            assert_eq!(
                words,
                BTreeSet::from([vertex_diffuse(vertex)]),
                "{id:?}: the mesh does not carry surface alpha conversion's own curr_alpha"
            );

            // The runtime channel, on the same part, at a translucency of its own.
            let part_t = 0.25f32;
            let factor = material_texture_factor(Some(MaterialOverride {
                translucency: part_t,
                diffuse: dereth_animation::parts::DEFAULT_DIFFUSE,
                luminosity: dereth_animation::parts::DEFAULT_LUMINOSITY,
            }))
            .expect("t = 0.25 sets has_alpha");
            let material = (factor >> 24) as u8;
            let product = u8::try_from((u32::from(vertex) * u32::from(material)) / 255)
                .expect("a product of two bytes over 255");
            eprintln!(
                "Runtime alpha override: {id:?} is TRANSLUCENT at translucency {surface_t}, so surface conversion \
                 bakes vertex alpha {vertex:#04X}; a part at translucency {part_t} carries material \
                 alpha {material:#04X}; multiplying them would produce {product:#04X}"
            );
            assert_eq!(
                material,
                dereth_primitives::num::to_i32((1.0 - part_t) * 255.0).clamp(0, 255) as u8,
                "the material alpha is not 1 - t"
            );
            assert_ne!(
                material, vertex,
                "the two channels are indistinguishable on this surface"
            );
            assert_ne!(
                material, product,
                "a multiply and an override agree here, so this proves nothing"
            );
            // `draw_part` submits `factor` with `draw_params.w = 1`; the shader's
            // `lerp(i.color, g_textureFactor, 1.0)` is the texture factor and nothing else, so the
            // baked word above is discarded rather than modulated.
            assert_eq!(
                factor & 0x00FF_FFFF,
                0x00FF_FFFF,
                "the substituted RGB is not white"
            );
        }

        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn every_surface_in_the_dat_takes_row_thirteen_exactly_once() {
            use dereth_render::pso::Blend;

            let store = retail_store();
            let ctx = SurfaceContext {
                vertex_format: VertexFormat::XyzNormalDiffuseTex1,
                texture_is_set: true,
                lighting: true,
                ..SurfaceContext::default()
            };

            let (mut total, mut moved, mut spared) = (0usize, 0usize, 0usize);
            let mut sample: Vec<DataId> = Vec::new();
            for id in store.ids_of(DbType::Surface) {
                let Some(s) = read_surface(&store, id) else {
                    continue;
                };
                let state = RenderState {
                    r#type: s.surface_type,
                    handler: SurfaceHandler::Database,
                    color_value: s.color_value.unwrap_or(0),
                    translucency: s.translucency,
                    luminosity: s.luminosity,
                    diffuse: s.diffuse,
                };
                total += 1;
                let base = PipelineKey::state_from_surface(&state, ctx).key;
                let over = PipelineKey::state_from_surface(
                    &state,
                    SurfaceContext {
                        material_has_alpha: Some(true),
                        ..ctx
                    },
                )
                .key;
                if sample.len() < 48 {
                    sample.push(id);
                }
                if base.alpha_blend && !base.alpha_test {
                    // Row 13's first branch: already blending without a test, left alone. This is
                    // what spares an `Additive` glow its `ONE / ONE`.
                    assert_eq!(
                        over, base,
                        "{id:?} was changed by an override that must not fire"
                    );
                    spared += 1;
                } else {
                    assert_eq!(over.src_blend, Blend::SrcAlpha, "{id:?}");
                    assert_eq!(over.dst_blend, Blend::InvSrcAlpha, "{id:?}");
                    assert!(
                        over.alpha_blend && !over.alpha_test && !over.z_write,
                        "{id:?}"
                    );
                    // Nothing *else* may move: it is the same surface, same texture, same cull.
                    assert_eq!(
                        (
                            over.vertex_format,
                            over.cull,
                            over.stage_ops,
                            over.fog,
                            over.lighting
                        ),
                        (
                            base.vertex_format,
                            base.cull,
                            base.stage_ops,
                            base.fog,
                            base.lighting
                        ),
                        "{id:?}: the override changed more than row 13's five fields"
                    );
                    moved += 1;
                }
            }
            eprintln!(
                "Material alpha selection: {total} surfaces resolved; the override \
                 changes {moved} and spares {spared}"
            );
            assert!(total > 1000, "only {total} surfaces resolved");
            assert!(
                moved > 0,
                "the override fired on nothing, so this test asserts nothing"
            );
            assert!(
                spared > 0,
                "the override fired on everything; the first branch is untested"
            );

            // **And the wire.** The sweep above is over `state_from_surface` directly; this is the
            // path a part actually takes — [`resolve_surface`] and [`build_meshes`] — asserted to
            // carry the *same* two keys onto the [`PartMesh`]. A sample rather than the whole dat
            // because each resolve uploads a texture and the descriptor heap is finite.
            let mut gpu = warp_gpu();
            let textures = TextureStore::new(&store);
            let mut cache = BakeCache::default();
            let mut wired = 0usize;
            for id in &sample {
                let Some(s) = read_surface(&store, *id) else {
                    continue;
                };
                let meshes = build_meshes(
                    &store,
                    &mut cache,
                    &textures,
                    &mut gpu,
                    &one_triangle(*id),
                    None,
                    None,
                )
                .expect("the group resolves");
                let state = RenderState {
                    r#type: s.surface_type,
                    handler: SurfaceHandler::Database,
                    color_value: s.color_value.unwrap_or(0),
                    translucency: s.translucency,
                    luminosity: s.luminosity,
                    diffuse: s.diffuse,
                };
                // `texture_is_set` follows whichever way the texture actually resolved, so take
                // the base key from the mesh and only assert that the *override* is row 13 of it.
                for m in &meshes {
                    let want = PipelineKey::state_from_surface(
                        &state,
                        SurfaceContext {
                            material_has_alpha: Some(true),
                            texture_is_set: m.texture.is_some(),
                            ..ctx
                        },
                    )
                    .key;
                    assert_eq!(
                        m.key_material_alpha, want,
                        "{id:?}: PartMesh::key_material_alpha is not surface alpha conversion's row 13"
                    );
                    assert!(
                        !m.key_material_alpha.z_write,
                        "{id:?}: row 13 always turns the depth write off"
                    );
                    wired += 1;
                }
            }
            eprintln!(
                "Mesh material channels: {wired} PartMesh(es) over {} sampled surfaces",
                sample.len()
            );
            assert!(
                wired > 0,
                "build_meshes emitted nothing, so the wire is untested"
            );
        }
    }

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

    /// The pair's methods on the shared view, so a reader holding the application's
    /// world state and the renderer's drawing half asks them exactly as it asked a `WorldScene`.
    impl WorldSceneRef<'_> {
        /// [`SceneDraw::world_fog_state`] on this view.
        pub fn world_fog_state(&self) -> dereth_render::camera::FogParams {
            self.draw.world_fog_state(self.world)
        }

        /// [`SceneDraw::world_fog`] on this view.
        pub fn world_fog(&self) -> dereth_render::camera::FogParams {
            self.draw.world_fog()
        }

        /// [`SceneDraw::detail_texturing`] on this view.
        pub fn detail_texturing(&self) -> dereth_world_render::detail::DetailTexturing {
            self.draw.detail_texturing()
        }

        /// [`SceneDraw::live_appearances`] on this view.
        pub fn live_appearances(&self) -> usize {
            self.draw.live_appearances()
        }

        /// [`SceneDraw::clipmap_conflicts`] on this view.
        pub fn clipmap_conflicts(&self) -> &[ClipMapConflict] {
            self.draw.clipmap_conflicts()
        }

        /// [`SceneDraw::appearance_link_counts`] on this view.
        pub fn appearance_link_counts(&self) -> Vec<usize> {
            self.draw.appearance_link_counts()
        }

        /// [`SceneDraw::cell_static_batches`] on this view.
        pub fn cell_static_batches(&self, cell: CellId) -> usize {
            self.draw.cell_static_batches(cell)
        }

        /// [`SceneDraw::resident_blocks`] on this view.
        pub fn resident_blocks(&self) -> usize {
            self.draw.resident_blocks()
        }

        /// [`SceneDraw::pending_slot_count`] on this view.
        pub fn pending_slot_count(&self) -> usize {
            self.draw.pending_slot_count()
        }

        /// [`SceneDraw::released_block_count`] on this view.
        pub fn released_block_count(&self) -> usize {
            self.draw.released_block_count()
        }

        /// [`SceneDraw::last_terrain_surface_built`] on this view.
        pub fn last_terrain_surface_built(&self) -> Option<(u32, u32, u32)> {
            self.draw.last_terrain_surface_built()
        }

        /// [`SceneDraw::last_object_texture_built`] on this view.
        pub fn last_object_texture_built(&self) -> Option<(u32, u32, u32)> {
            self.draw.last_object_texture_built()
        }

        /// [`SceneDraw::object_texture_census`] on this view.
        pub fn object_texture_census(&self) -> (u64, u64, u64, u64) {
            self.draw.object_texture_census()
        }

        /// [`SceneDraw::render_shadow`] on this view.
        pub fn render_shadow(&self) -> crate::render_prefs::RenderPreferences {
            self.draw.render_shadow()
        }

        /// [`SceneDraw::block_bake`] on this view.
        pub fn block_bake(&self, block: (i32, i32)) -> Option<BlockBake> {
            self.draw.block_bake(block)
        }

        /// [`SceneDraw::window_rings`] on this view.
        pub fn window_rings(&self) -> Vec<WindowRing> {
            self.draw.window_rings(self.world)
        }

        /// [`SceneDraw::alpha_list_batches`] on this view.
        pub fn alpha_list_batches(&self) -> usize {
            self.draw.alpha_list_batches()
        }

        /// [`SceneDraw::part_degrade_probe`] on this view.
        pub fn part_degrade_probe(&self) -> Vec<PartLevelProbe> {
            self.draw.part_degrade_probe(self.world)
        }

        /// [`SceneDraw::degrade_probe`] on this view.
        pub fn degrade_probe(&self) -> Vec<StaticLevelProbe> {
            self.draw.degrade_probe(self.world)
        }

        /// [`SceneDraw::force_level`] on this view.
        pub fn force_level(&self) -> i32 {
            self.draw.force_level()
        }

        /// [`SceneDraw::host_pending_events`] on this view.
        pub fn host_pending_events(&self) -> usize {
            self.draw.host_pending_events()
        }

        /// [`SceneDraw::host_bodies`] on this view.
        pub fn host_bodies(&self) -> Vec<HostBody> {
            self.draw.host_bodies(self.world)
        }

        /// [`SceneDraw::region`] on this view.
        pub fn region(&self) -> &dereth_assets::Region {
            self.draw.region()
        }

        /// [`SceneDraw::character_built_from`] on this view.
        pub fn character_built_from(&self) -> &[DataId] {
            self.draw.character_built_from()
        }

        /// [`SceneDraw::character_part_textures`] on this view.
        pub fn character_part_textures(&self) -> Vec<Vec<u32>> {
            self.draw.character_part_textures()
        }

        /// [`SceneDraw::terrain_neighbourhood`] on this view.
        pub fn terrain_neighbourhood(&self) -> dereth_audio::TerrainNeighbourhood {
            self.draw.terrain_neighbourhood(self.world)
        }

        /// [`SceneDraw::target_projection`] on this view.
        pub fn target_projection(
            &self,
            id: ObjectId,
            viewport: (u32, u32),
        ) -> Option<dereth_client_contract::target::Projection> {
            self.draw.target_projection(self.world, id, viewport)
        }

        /// [`SceneDraw::upload_reservation`] on this view.
        pub fn upload_reservation(&self) -> usize {
            self.draw.upload_reservation()
        }

        /// [`SceneDraw::reserve_upload_arena`] on this view.
        pub fn reserve_upload_arena(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            self.draw.reserve_upload_arena(gpu)
        }

        /// [`SceneDraw::degrade_globals`] on this view.
        pub fn degrade_globals(&self) -> dereth_world_render::objects::degrade::DegradeGlobals {
            self.draw.degrade_globals(self.world)
        }

        /// [`SceneDraw::weather_enabled`] on this view.
        pub fn weather_enabled(&self) -> Option<bool> {
            self.draw.weather_enabled()
        }

        /// [`SceneDraw::landscape_lighting`] on this view.
        pub fn landscape_lighting(&self) -> LandscapeLighting {
            self.draw.landscape_lighting()
        }

        /// [`SceneDraw::viewer_block_colours`] on this view.
        #[must_use]
        pub fn viewer_block_colours(&self) -> Option<Vec<[u8; 3]>> {
            self.draw.viewer_block_colours(self.world)
        }

        /// [`SceneDraw::view_params`] on this view.
        pub fn view_params(&self, width: u32, height: u32) -> ViewParams {
            self.draw.view_params(self.world, width, height)
        }

        /// [`SceneDraw::effective_viewport`] on this view.
        pub fn effective_viewport(
            &self,
            width: u32,
            height: u32,
        ) -> dereth_render::camera::Viewport {
            self.draw.effective_viewport(self.world, width, height)
        }

        /// [`SceneDraw::set_selected_object_id`] on this view.
        pub fn set_selected_object_id(&self, id: Option<ObjectId>) {
            self.draw.set_selected_object_id(id)
        }

        /// [`SceneDraw::take_selected_part_drawn`] on this view.
        pub fn take_selected_part_drawn(&self) -> bool {
            self.draw.take_selected_part_drawn()
        }

        /// [`SceneDraw::clear_selected_part_drawn`] on this view.
        pub fn clear_selected_part_drawn(&self) {
            self.draw.clear_selected_part_drawn()
        }

        /// [`SceneDraw::sun_light`] on this view.
        pub fn sun_light(&self) -> Option<D3dLight> {
            self.draw.sun_light()
        }

        /// [`SceneDraw::world_ambient_color`] on this view.
        pub fn world_ambient_color(&self) -> [f32; 3] {
            self.draw.world_ambient_color(self.world)
        }

        /// [`SceneDraw::object_light_set`] on this view.
        pub fn object_light_set(&self, centre: Vec3, radius: f32, outdoors: bool) -> Vec<D3dLight> {
            self.draw.object_light_set(centre, radius, outdoors)
        }

        /// [`SceneDraw::envcell_light_set`] on this view.
        pub fn envcell_light_set(&self) -> Vec<D3dLight> {
            self.draw.envcell_light_set()
        }

        /// [`SceneDraw::light_pools`] on this view.
        pub fn light_pools(&self) -> &LightPools {
            self.draw.light_pools()
        }

        /// [`SceneDraw::cell_light_objects`] on this view.
        pub fn cell_light_objects(&self) -> usize {
            self.draw.cell_light_objects()
        }

        /// [`SceneDraw::env_cell_burned_vertices`] on this view.
        pub fn env_cell_burned_vertices(&self, cell: CellId) -> Vec<(Vec3, Vec3, [u8; 3])> {
            self.draw.env_cell_burned_vertices(cell)
        }

        /// [`SceneDraw::env_cell_static_light_probe`] on this view.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        pub fn env_cell_static_light_probe(
            &self,
            cell: CellId,
        ) -> Vec<(Vec3, f32, usize, usize, usize, f32, bool, bool)> {
            self.draw.env_cell_static_light_probe(cell)
        }

        /// [`SceneDraw::cell_block_origin`] on this view.
        pub fn cell_block_origin(&self, cell: CellId) -> Option<(f32, f32)> {
            self.draw.cell_block_origin(cell)
        }

        /// [`SceneDraw::draw`] on this view.
        pub fn draw(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            self.draw.draw(self.world, gpu)
        }

        /// [`SceneDraw::emitter_degrade_probe`] on this view.
        pub fn emitter_degrade_probe(&self) -> Vec<EmitterDegrade> {
            self.draw.emitter_degrade_probe(self.world)
        }

        /// [`SceneDraw::emitter_host_origins`] on this view.
        pub fn emitter_host_origins(&self) -> Vec<Vec3> {
            self.draw.emitter_host_origins(self.world)
        }

        /// [`SceneDraw::drawn_particles`] on this view.
        pub fn drawn_particles(&self) -> crate::particles::ParticleStats {
            self.draw.drawn_particles()
        }

        /// [`SceneDraw::drawn_material_parts`] on this view.
        pub fn drawn_material_parts(&self) -> (u32, u32) {
            self.draw.drawn_material_parts()
        }

        /// [`SceneDraw::drawn_alpha_lists`] on this view.
        pub fn drawn_alpha_lists(&self) -> AlphaListStats {
            self.draw.drawn_alpha_lists()
        }

        /// [`SceneDraw::drawn_landscape_alpha`] on this view.
        pub fn drawn_landscape_alpha(&self) -> LandscapeAlphaStats {
            self.draw.drawn_landscape_alpha()
        }

        /// [`SceneDraw::drawn_alpha_order`] on this view.
        pub fn drawn_alpha_order(&self) -> Vec<AlphaDraw> {
            self.draw.drawn_alpha_order()
        }

        /// [`SceneDraw::drawn_blend_order`] on this view.
        pub fn drawn_blend_order(&self) -> Vec<(AlphaDraw, f32)> {
            self.draw.drawn_blend_order()
        }

        /// [`SceneDraw::drawn_object_cone`] on this view.
        pub fn drawn_object_cone(&self) -> ObjectConeStats {
            self.draw.drawn_object_cone()
        }

        /// [`SceneDraw::drawn_part_order`] on this view.
        pub fn drawn_part_order(&self) -> Vec<PartSubsetDraw> {
            self.draw.drawn_part_order()
        }

        /// [`SceneDraw::drawn_cells`] on this view.
        pub fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
            self.draw.drawn_cells()
        }

        /// [`SceneDraw::drawn_objects`] on this view.
        pub fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
            self.draw.drawn_objects()
        }

        /// [`SceneDraw::outside_view_polys`] on this view.
        pub fn outside_view_polys(&self) -> Vec<Vec<(f32, f32)>> {
            self.draw.outside_view_polys()
        }

        /// [`SceneDraw::drawn_portal_stamps`] on this view.
        pub fn drawn_portal_stamps(&self) -> (u64, u64) {
            self.draw.drawn_portal_stamps()
        }

        /// [`SceneDraw::cell_view_polys`] on this view.
        pub fn cell_view_polys(&self, cell: CellId) -> Vec<Vec<(f32, f32)>> {
            self.draw.cell_view_polys(cell)
        }

        /// [`SceneDraw::building_portal_openings`] on this view.
        pub fn building_portal_openings(&self) -> Vec<(Vec3, Vec3)> {
            self.draw.building_portal_openings()
        }

        /// [`SceneDraw::building_portal_screen_polygons`] on this view.
        pub fn building_portal_screen_polygons(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            self.draw
                .building_portal_screen_polygons(self.world, width, height)
        }

        /// [`SceneDraw::indoor_outdoor_portal_screen_polygons`] on this view.
        pub fn indoor_outdoor_portal_screen_polygons(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            self.draw
                .indoor_outdoor_portal_screen_polygons(self.world, width, height)
        }

        /// [`SceneDraw::cell_static_screen_boxes`] on this view.
        pub fn cell_static_screen_boxes(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<(f32, f32, f32, f32)> {
            self.draw
                .cell_static_screen_boxes(self.world, width, height)
        }

        /// [`SceneDraw::indoor_traversal_counts`] on this view.
        pub fn indoor_traversal_counts(&self) -> (usize, usize) {
            self.draw.indoor_traversal_counts(self.world)
        }

        /// [`SceneDraw::indoor_outside_view_count`] on this view.
        pub fn indoor_outside_view_count(&self) -> Option<usize> {
            self.draw.indoor_outside_view_count(self.world)
        }

        /// [`SceneDraw::env_cell_counts`] on this view.
        pub fn env_cell_counts(&self) -> (usize, usize) {
            self.draw.env_cell_counts(self.world)
        }
    }

    /// …and every one of them on the mutable view.
    impl WorldSceneMut<'_> {
        /// [`SceneDraw::world_fog_state`] on this view.
        pub fn world_fog_state(&self) -> dereth_render::camera::FogParams {
            self.draw.world_fog_state(&*self.world)
        }

        /// [`SceneDraw::world_fog`] on this view.
        pub fn world_fog(&self) -> dereth_render::camera::FogParams {
            self.draw.world_fog()
        }

        /// [`SceneDraw::stream`] on this view.
        pub fn stream(&mut self, store: &RetailDatStore, gpu: &mut Gpu) -> Result<(), WorldError> {
            self.draw.stream(self.world, store, gpu)
        }

        /// [`SceneDraw::update_from_preferences`] on this view.
        pub fn update_from_preferences(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
        ) -> Result<RenderPrefWork, WorldError> {
            self.draw.update_from_preferences(self.world, store, gpu)
        }

        /// [`SceneDraw::detail_texturing`] on this view.
        pub fn detail_texturing(&self) -> dereth_world_render::detail::DetailTexturing {
            self.draw.detail_texturing()
        }

        /// [`SceneDraw::release_textures`] on this view.
        pub fn release_textures(&mut self, gpu: &mut Gpu) -> u32 {
            self.draw.release_textures(gpu)
        }

        /// [`SceneDraw::live_appearances`] on this view.
        pub fn live_appearances(&self) -> usize {
            self.draw.live_appearances()
        }

        /// [`SceneDraw::clipmap_conflicts`] on this view.
        pub fn clipmap_conflicts(&self) -> &[ClipMapConflict] {
            self.draw.clipmap_conflicts()
        }

        /// [`SceneDraw::appearance_link_counts`] on this view.
        pub fn appearance_link_counts(&self) -> Vec<usize> {
            self.draw.appearance_link_counts()
        }

        /// [`SceneDraw::cell_static_batches`] on this view.
        pub fn cell_static_batches(&self, cell: CellId) -> usize {
            self.draw.cell_static_batches(cell)
        }

        /// [`SceneDraw::resident_blocks`] on this view.
        pub fn resident_blocks(&self) -> usize {
            self.draw.resident_blocks()
        }

        /// [`SceneDraw::pending_slot_count`] on this view.
        pub fn pending_slot_count(&self) -> usize {
            self.draw.pending_slot_count()
        }

        /// [`SceneDraw::released_block_count`] on this view.
        pub fn released_block_count(&self) -> usize {
            self.draw.released_block_count()
        }

        /// [`SceneDraw::last_terrain_surface_built`] on this view.
        pub fn last_terrain_surface_built(&self) -> Option<(u32, u32, u32)> {
            self.draw.last_terrain_surface_built()
        }

        /// [`SceneDraw::last_object_texture_built`] on this view.
        pub fn last_object_texture_built(&self) -> Option<(u32, u32, u32)> {
            self.draw.last_object_texture_built()
        }

        /// [`SceneDraw::object_texture_census`] on this view.
        pub fn object_texture_census(&self) -> (u64, u64, u64, u64) {
            self.draw.object_texture_census()
        }

        /// [`SceneDraw::render_shadow`] on this view.
        pub fn render_shadow(&self) -> crate::render_prefs::RenderPreferences {
            self.draw.render_shadow()
        }

        /// [`SceneDraw::block_bake`] on this view.
        pub fn block_bake(&self, block: (i32, i32)) -> Option<BlockBake> {
            self.draw.block_bake(block)
        }

        /// [`SceneDraw::window_rings`] on this view.
        pub fn window_rings(&self) -> Vec<WindowRing> {
            self.draw.window_rings(&*self.world)
        }

        /// [`SceneDraw::alpha_list_batches`] on this view.
        pub fn alpha_list_batches(&self) -> usize {
            self.draw.alpha_list_batches()
        }

        /// [`SceneDraw::part_degrade_probe`] on this view.
        pub fn part_degrade_probe(&self) -> Vec<PartLevelProbe> {
            self.draw.part_degrade_probe(&*self.world)
        }

        /// [`SceneDraw::degrade_probe`] on this view.
        pub fn degrade_probe(&self) -> Vec<StaticLevelProbe> {
            self.draw.degrade_probe(&*self.world)
        }

        /// [`SceneDraw::set_force_level`] on this view.
        pub fn set_force_level(&mut self, level: i32) {
            self.draw.set_force_level(level)
        }

        /// [`SceneDraw::force_level`] on this view.
        pub fn force_level(&self) -> i32 {
            self.draw.force_level()
        }

        /// [`SceneDraw::attach_character`] on this view.
        pub fn attach_character(
            &mut self,
            store: &std::sync::Arc<RetailDatStore>,
            region: &dereth_assets::Region,
            gpu: &mut Gpu,
        ) -> Result<(), WorldError> {
            self.draw.attach_character(self.world, store, region, gpu)
        }

        /// [`SceneDraw::sync_objects`] on this view.
        pub fn sync_objects(
            &mut self,
            store: &Arc<RetailDatStore>,
            gpu: &mut Gpu,
            stream: &mut ObjectStream,
        ) -> Result<(), WorldError> {
            self.draw.sync_objects(self.world, store, gpu, stream)
        }

        /// [`SceneDraw::host_pending_events`] on this view.
        pub fn host_pending_events(&self) -> usize {
            self.draw.host_pending_events()
        }

        /// [`SceneDraw::host_bodies`] on this view.
        pub fn host_bodies(&self) -> Vec<HostBody> {
            self.draw.host_bodies(&*self.world)
        }

        /// [`SceneDraw::region`] on this view.
        pub fn region(&self) -> &dereth_assets::Region {
            self.draw.region()
        }

        /// [`SceneDraw::character_built_from`] on this view.
        pub fn character_built_from(&self) -> &[DataId] {
            self.draw.character_built_from()
        }

        /// [`SceneDraw::character_part_textures`] on this view.
        pub fn character_part_textures(&self) -> Vec<Vec<u32>> {
            self.draw.character_part_textures()
        }

        /// [`SceneDraw::terrain_neighbourhood`] on this view.
        pub fn terrain_neighbourhood(&self) -> dereth_audio::TerrainNeighbourhood {
            self.draw.terrain_neighbourhood(&*self.world)
        }

        /// [`SceneDraw::take_sound_events`] on this view.
        pub fn take_sound_events(&mut self) -> Vec<SoundTrigger> {
            self.draw.take_sound_events(self.world)
        }

        /// [`SceneDraw::replace_player_particle_script`] on this view.
        pub fn replace_player_particle_script(&mut self, script: DataId) -> bool {
            self.draw.replace_player_particle_script(self.world, script)
        }

        /// [`SceneDraw::process_hooks`] on this view.
        pub fn process_hooks(&mut self) {
            self.draw.process_hooks(self.world)
        }

        /// [`SceneDraw::target_projection`] on this view.
        pub fn target_projection(
            &self,
            id: ObjectId,
            viewport: (u32, u32),
        ) -> Option<dereth_client_contract::target::Projection> {
            self.draw.target_projection(&*self.world, id, viewport)
        }

        /// [`SceneDraw::upload_reservation`] on this view.
        pub fn upload_reservation(&self) -> usize {
            self.draw.upload_reservation()
        }

        /// [`SceneDraw::reserve_upload_arena`] on this view.
        pub fn reserve_upload_arena(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            self.draw.reserve_upload_arena(gpu)
        }

        /// [`SceneDraw::update`] on this view.
        pub fn update(
            &mut self,
            input: crate::camera::CameraInput,
            character: dereth_client_runtime::character::CharacterInput,
            now: dereth_primitives::LocalTime,
            dt: f32,
        ) {
            self.draw.update(self.world, input, character, now, dt)
        }

        /// [`SceneDraw::degrade_globals`] on this view.
        pub fn degrade_globals(&self) -> dereth_world_render::objects::degrade::DegradeGlobals {
            self.draw.degrade_globals(&*self.world)
        }

        /// [`SceneDraw::set_weather_enabled`] on this view.
        pub fn set_weather_enabled(&mut self, on: bool) {
            self.draw.set_weather_enabled(on)
        }

        /// [`SceneDraw::weather_enabled`] on this view.
        pub fn weather_enabled(&self) -> Option<bool> {
            self.draw.weather_enabled()
        }

        /// [`SceneDraw::set_world_fog`] on this view.
        pub fn set_world_fog(&mut self, on: bool) -> bool {
            self.draw.set_world_fog(self.world, on)
        }

        /// [`SceneDraw::set_always_daylight`] on this view.
        pub fn set_always_daylight(&mut self, on: bool) -> bool {
            self.draw.set_always_daylight(self.world, on)
        }

        /// [`SceneDraw::landscape_lighting`] on this view.
        pub fn landscape_lighting(&self) -> LandscapeLighting {
            self.draw.landscape_lighting()
        }

        /// [`SceneDraw::viewer_block_colours`] on this view.
        #[must_use]
        pub fn viewer_block_colours(&self) -> Option<Vec<[u8; 3]>> {
            self.draw.viewer_block_colours(self.world)
        }

        /// [`SceneDraw::apply_camera_translucency`] on this view.
        pub fn apply_camera_translucency(&mut self) {
            self.draw.apply_camera_translucency(self.world)
        }

        /// [`SceneDraw::view_params`] on this view.
        pub fn view_params(&self, width: u32, height: u32) -> ViewParams {
            self.draw.view_params(&*self.world, width, height)
        }

        /// [`SceneDraw::set_game_viewport`] on this view.
        pub fn set_game_viewport(&mut self, viewport: Option<dereth_render::camera::Viewport>) {
            self.draw.set_game_viewport(viewport)
        }

        /// [`SceneDraw::effective_viewport`] on this view.
        pub fn effective_viewport(
            &self,
            width: u32,
            height: u32,
        ) -> dereth_render::camera::Viewport {
            self.draw.effective_viewport(&*self.world, width, height)
        }

        /// [`SceneDraw::set_selected_object_id`] on this view.
        pub fn set_selected_object_id(&self, id: Option<ObjectId>) {
            self.draw.set_selected_object_id(id)
        }

        /// [`SceneDraw::take_selected_part_drawn`] on this view.
        pub fn take_selected_part_drawn(&self) -> bool {
            self.draw.take_selected_part_drawn()
        }

        /// [`SceneDraw::clear_selected_part_drawn`] on this view.
        pub fn clear_selected_part_drawn(&self) {
            self.draw.clear_selected_part_drawn()
        }

        /// [`SceneDraw::sun_light`] on this view.
        pub fn sun_light(&self) -> Option<D3dLight> {
            self.draw.sun_light()
        }

        /// [`SceneDraw::world_ambient_color`] on this view.
        pub fn world_ambient_color(&self) -> [f32; 3] {
            self.draw.world_ambient_color(&*self.world)
        }

        /// [`SceneDraw::object_light_set`] on this view.
        pub fn object_light_set(&self, centre: Vec3, radius: f32, outdoors: bool) -> Vec<D3dLight> {
            self.draw.object_light_set(centre, radius, outdoors)
        }

        /// [`SceneDraw::envcell_light_set`] on this view.
        pub fn envcell_light_set(&self) -> Vec<D3dLight> {
            self.draw.envcell_light_set()
        }

        /// [`SceneDraw::light_pools`] on this view.
        pub fn light_pools(&self) -> &LightPools {
            self.draw.light_pools()
        }

        /// [`SceneDraw::cell_light_objects`] on this view.
        pub fn cell_light_objects(&self) -> usize {
            self.draw.cell_light_objects()
        }

        /// [`SceneDraw::env_cell_burned_vertices`] on this view.
        pub fn env_cell_burned_vertices(&self, cell: CellId) -> Vec<(Vec3, Vec3, [u8; 3])> {
            self.draw.env_cell_burned_vertices(cell)
        }

        /// [`SceneDraw::env_cell_static_light_probe`] on this view.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        pub fn env_cell_static_light_probe(
            &self,
            cell: CellId,
        ) -> Vec<(Vec3, f32, usize, usize, usize, f32, bool, bool)> {
            self.draw.env_cell_static_light_probe(cell)
        }

        /// [`SceneDraw::cell_block_origin`] on this view.
        pub fn cell_block_origin(&self, cell: CellId) -> Option<(f32, f32)> {
            self.draw.cell_block_origin(cell)
        }

        /// [`SceneDraw::draw`] on this view.
        pub fn draw(&self, gpu: &mut Gpu) -> Result<(), RenderError> {
            self.draw.draw(&*self.world, gpu)
        }

        /// [`SceneDraw::emitter_degrade_probe`] on this view.
        pub fn emitter_degrade_probe(&self) -> Vec<EmitterDegrade> {
            self.draw.emitter_degrade_probe(&*self.world)
        }

        /// [`SceneDraw::emitter_host_origins`] on this view.
        pub fn emitter_host_origins(&self) -> Vec<Vec3> {
            self.draw.emitter_host_origins(&*self.world)
        }

        /// [`SceneDraw::drawn_particles`] on this view.
        pub fn drawn_particles(&self) -> crate::particles::ParticleStats {
            self.draw.drawn_particles()
        }

        /// [`SceneDraw::drawn_material_parts`] on this view.
        pub fn drawn_material_parts(&self) -> (u32, u32) {
            self.draw.drawn_material_parts()
        }

        /// [`SceneDraw::drawn_alpha_lists`] on this view.
        pub fn drawn_alpha_lists(&self) -> AlphaListStats {
            self.draw.drawn_alpha_lists()
        }

        /// [`SceneDraw::drawn_landscape_alpha`] on this view.
        pub fn drawn_landscape_alpha(&self) -> LandscapeAlphaStats {
            self.draw.drawn_landscape_alpha()
        }

        /// [`SceneDraw::drawn_alpha_order`] on this view.
        pub fn drawn_alpha_order(&self) -> Vec<AlphaDraw> {
            self.draw.drawn_alpha_order()
        }

        /// [`SceneDraw::drawn_blend_order`] on this view.
        pub fn drawn_blend_order(&self) -> Vec<(AlphaDraw, f32)> {
            self.draw.drawn_blend_order()
        }

        /// [`SceneDraw::drawn_object_cone`] on this view.
        pub fn drawn_object_cone(&self) -> ObjectConeStats {
            self.draw.drawn_object_cone()
        }

        /// [`SceneDraw::drawn_part_order`] on this view.
        pub fn drawn_part_order(&self) -> Vec<PartSubsetDraw> {
            self.draw.drawn_part_order()
        }

        /// [`SceneDraw::drawn_cells`] on this view.
        pub fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
            self.draw.drawn_cells()
        }

        /// [`SceneDraw::drawn_objects`] on this view.
        pub fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
            self.draw.drawn_objects()
        }

        /// [`SceneDraw::outside_view_polys`] on this view.
        pub fn outside_view_polys(&self) -> Vec<Vec<(f32, f32)>> {
            self.draw.outside_view_polys()
        }

        /// [`SceneDraw::drawn_portal_stamps`] on this view.
        pub fn drawn_portal_stamps(&self) -> (u64, u64) {
            self.draw.drawn_portal_stamps()
        }

        /// [`SceneDraw::cell_view_polys`] on this view.
        pub fn cell_view_polys(&self, cell: CellId) -> Vec<Vec<(f32, f32)>> {
            self.draw.cell_view_polys(cell)
        }

        /// [`SceneDraw::building_portal_openings`] on this view.
        pub fn building_portal_openings(&self) -> Vec<(Vec3, Vec3)> {
            self.draw.building_portal_openings()
        }

        /// [`SceneDraw::building_portal_screen_polygons`] on this view.
        pub fn building_portal_screen_polygons(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            self.draw
                .building_portal_screen_polygons(&*self.world, width, height)
        }

        /// [`SceneDraw::indoor_outdoor_portal_screen_polygons`] on this view.
        pub fn indoor_outdoor_portal_screen_polygons(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<Vec<(f32, f32)>> {
            self.draw
                .indoor_outdoor_portal_screen_polygons(&*self.world, width, height)
        }

        /// [`SceneDraw::cell_static_screen_boxes`] on this view.
        pub fn cell_static_screen_boxes(
            &self,
            width: u32,
            height: u32,
        ) -> Vec<(f32, f32, f32, f32)> {
            self.draw
                .cell_static_screen_boxes(&*self.world, width, height)
        }

        /// [`SceneDraw::indoor_traversal_counts`] on this view.
        pub fn indoor_traversal_counts(&self) -> (usize, usize) {
            self.draw.indoor_traversal_counts(&*self.world)
        }

        /// [`SceneDraw::indoor_outside_view_count`] on this view.
        pub fn indoor_outside_view_count(&self) -> Option<usize> {
            self.draw.indoor_outside_view_count(&*self.world)
        }

        /// [`SceneDraw::env_cell_counts`] on this view.
        pub fn env_cell_counts(&self) -> (usize, usize) {
            self.draw.env_cell_counts(&*self.world)
        }
    }

    /// The two halves of a scene, however they are held. What the three `Scene` impls below read
    /// through, so they are one text.
    pub(crate) trait SceneHalves {
        fn halves(&self) -> (&WorldState, &SceneDraw);
    }

    /// …and mutably, for the writes.
    pub(crate) trait SceneHalvesMut: SceneHalves {
        fn halves_mut(&mut self) -> (&mut WorldState, &mut SceneDraw);
    }

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
    // forward: the scene's own inherent methods are unchanged and the traits exist so that `App`
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

                fn render_preferences(&self) -> crate::render_prefs::RenderPreferences {
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

                fn clear_selected_part_drawn(&self) {
                    self.halves().1.clear_selected_part_drawn();
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
                    input: crate::camera::CameraInput,
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
}
