//! Particles into the frame.
//!
//! This module preserves what reaches the GPU, the sorting and blend table, emitter simulation,
//! closed-form trajectories, shipped oddities, and viewer-distance LOD selection.
//!
//! **Particles are not a special renderer.** `dereth_animation::particles` already owns every emitter and
//! every particle; `dereth_world_render::objects::degrade` already owns the billboarding; this
//! module's job is to run the first once per frame and draw the result through the same graphics-object path
//! every other object uses. Nothing here evaluates a trajectory or draws a random number.
//!
//! What this module does own, because it is the *wiring*:
//!
//! * the per-emitter geometry cache keyed by `hw_gfxobj_id`, one mesh shared by every particle
//!   slot;
//! * the emitter's own degrade-distance draw cutoff;
//! * viewer-distance state for a particle part: depth, viewer heading, and selected draw frame;
//! * the draw order: parts far→near by depth, then the **clip list before the alpha list**;
//! * the translucency channel carried by the part's cloned material.

use std::collections::BTreeMap;

use dereth_animation::particles::ParticleManager;
use dereth_assets::motion::GfxObjDegradeInfo;
use dereth_assets::{Decode, GfxObj};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::PhysHandle;
use dereth_primitives::{CellId, DataId, Frame, Vec3};
use dereth_render::device::{Gpu, PerDrawConstants, PerFrameConstants};
use dereth_render::{DrawConstants, RenderError};
use dereth_world_render::degrade_loop::DegradeLevel;
use dereth_world_render::lighting::D3dLight;
use dereth_world_render::objects::degrade::{
    calc_draw_frame, get_degrade, DegradeGlobals, DegradeMode,
};
use {dereth_terrain::math::l2g, dereth_terrain::math::localtoglobalvec, dereth_terrain::math::V3};

use {crate::world_scene::world_constants_scaled, crate::world_scene::PartMesh};

/// Maximum degrade distance when the graphics object has no degrade record.
///
/// The same constant `dereth_world_render::particles::DEFAULT_PARTICLE_DEGRADE_DISTANCE` names; it is
/// re-exported here rather than re-derived so the two can never drift.
pub const DEFAULT_DEGRADE_DISTANCE: f32 =
    dereth_world_render::particles::DEFAULT_PARTICLE_DEGRADE_DISTANCE;

/// One emitter's drawable mesh and the LOD table that decides how it is billboarded.
///
/// Every particle of an emitter shares this: initialization makes `max_particles`
/// particle parts from the **one** `hw_gfxobj_id`, so the cache is keyed by that id and a screen
/// full of torches costs one entry.
#[derive(Debug)]
pub(crate) struct ParticleGfx {
    /// The triangles, in object space, one batch per surface.
    pub(crate) meshes: Vec<PartMesh>,
    /// The sort center used for viewer-distance measurements instead of the origin.
    pub(crate) sort_center: Vec3,
    /// The graphics object's optional degrade record.
    pub(crate) degrade: Option<GfxObjDegradeInfo>,
    /// The drawing sphere in the part's own unscaled space.
    ///
    /// The view-cone check scales it by the part's `gfxobj_scale`, places it
    /// through `draw_pos` and leaves the result as the object's local centre and radius for
    /// object-light minimization to read. So a particle's light set is chosen at the particle's own
    /// placed sphere, not at its emitter's, and that is what [`prepare`] rebuilds.
    ///
    /// `None` for a mesh with no drawing BSP, which falls back to the draw origin and radius zero
    /// exactly as `WorldScene::submission_light_set` does for a part without one.
    pub(crate) drawing_sphere: Option<(Vec3, f32)>,
}

/// The lit material a particle card is drawn through, as
/// [`dereth_render::device::PerDrawConstants::material_lighting`] wants it.
///
/// A particle part reaches D3D through either the default or cloned material path,
/// and the fixed-function diffuse and ambient coefficients are `(1, 1, 1)`
/// in both:
///
/// * At `translucency == 0`, the part restores its original material instead of cloning one, so the
///   part carries the graphics object's material, which is absent for every shipped graphics object
///   (the graphics-object record stores no material, which is why [`dereth_assets::GfxObj`]
///   has no such field). The absent-material path binds the default material and selects
///   the vertex as the diffuse and ambient colour source, so both come
///   from the vertex colour — which [`crate::world_scene::build_meshes`] writes white.
/// * Past zero it uses a cloned material. Its initialization clears the lighting values and then
///   writes ones to the diffuse and ambient components:
///   **Diffuse and Ambient white, Specular and Emissive zero**. The material path then
///   switches both colour sources from the vertex to the material.
///
/// Either way the vertex stage evaluates `saturate(Me + 1 * D3DRS_AMBIENT + 1 * sum)`, so this
/// constant states the one answer rather than tracking which of the two is bound this frame.
/// The alpha is a separate channel and is *not* folded in here: [`prepare`] already selects
/// the material as its source and carries `1 - t` in the texture factor's alpha byte.
///
/// `[Emissive.rgb, Diffuse.rgb, a material is bound, unused]`, the same shape
/// `WorldScene::material_lighting` produces for an animated part.
const PARTICLE_MATERIAL_LIGHTING: [f32; 4] = [
    dereth_animation::parts::DEFAULT_LUMINOSITY,
    dereth_animation::parts::DEFAULT_DIFFUSE,
    1.0,
    0.0,
];

/// How one drawn particle's light set is obtained:
/// at the particle's own placed drawing sphere, or from the sunlight set when its cell is outdoors.
///
/// A closure rather than a slice because the set is **per particle** — `viewcone_check` runs once
/// and `minimize_object_lighting` reads the globals it just wrote — and
/// `None` is `SceneConfig::object_lighting` off, which is the same `lights: None` the part path
/// takes and leaves `D3DRS_LIGHTING` clear.
pub(crate) type ParticleLightSet<'a> = &'a dyn Fn(Vec3, f32, bool) -> Vec<D3dLight>;

/// The geometry cache. `None` is cached too: a gfxobj with no drawing polygons is asked for once
/// per emitter; caching the miss avoids decoding the same absent geometry every frame.
#[derive(Debug, Default)]
pub(crate) struct ParticleGeometry {
    cache: BTreeMap<DataId, Option<ParticleGfx>>,
}

impl ParticleGeometry {
    /// Whether this id has already been resolved, hit or miss.
    pub(crate) fn contains(&self, id: DataId) -> bool {
        self.cache.contains_key(&id)
    }

    pub(crate) fn get(&self, id: DataId) -> Option<&ParticleGfx> {
        self.cache.get(&id).and_then(Option::as_ref)
    }

    /// How many distinct emitter meshes are live, for the counters.
    pub(crate) fn len(&self) -> usize {
        self.cache.values().filter(|v| v.is_some()).count()
    }

    /// Record a resolved entry.
    pub(crate) fn insert(&mut self, id: DataId, gfx: Option<ParticleGfx>) {
        self.cache.insert(id, gfx);
    }

    /// `degrades[n-2].max_dist`, or
    /// `degrades[0].max_dist` with two levels or fewer, and **100.0** with no degrade info at all.
    ///
    /// This is stored as `degrade_distance` and compared against viewer-space depth by the draw gate.
    ///
    /// The inner choice — `degrades[n-2]` or `degrades[0]` — is
    /// `dereth_world_render::objects::degrade::get_max_degrade_distance`, called rather than
    /// re-derived so there is one copy of the maximum-degrade-distance query. What this adds is
    /// the particle setup's `100.0` default when the part has no record at all.
    pub(crate) fn max_degrade_distance(&self, id: DataId) -> f32 {
        let Some(gfx) = self.get(id) else {
            return DEFAULT_DEGRADE_DISTANCE;
        };
        let Some(info) = gfx.degrade.as_ref() else {
            return DEFAULT_DEGRADE_DISTANCE;
        };
        if info.degrades.is_empty() {
            return DEFAULT_DEGRADE_DISTANCE;
        }
        dereth_world_render::objects::degrade::get_max_degrade_distance(info)
    }
}

/// Read one particle gfxobj's degrade record, if it names one.
pub(crate) fn read_degrade(store: &RetailDatStore, id: DataId) -> Option<GfxObjDegradeInfo> {
    let bytes = store.read_typed(DbType::GfxObj, id).ok()?;
    let obj = GfxObj::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
    let did = obj.did_degrade?;
    let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
    GfxObjDegradeInfo::decode_payload_in(store.era_of(did), did, &bytes).ok()
}

/// Read the point used to measure viewer distance.
pub(crate) fn read_sort_center(store: &RetailDatStore, id: DataId) -> Vec3 {
    let Ok(bytes) = store.read_typed(DbType::GfxObj, id) else {
        return Vec3::ZERO;
    };
    GfxObj::decode_payload_in(store.era_of(id), id, &bytes).map_or(Vec3::ZERO, |o| o.sort_center)
}

/// A placement whose setup record carries a `default_script`.
///
/// Static initialization turns every landblock-info placement into a physics object, and generated
/// scenery follows the same path. Setup creation queues the setup's `default_script_id` on the
/// object's script manager. That script is where a torch's
/// flame, a brazier's smoke and a lamp's glow come from: **32 of the static placements in the
/// 5×5 blocks around Holtburg name a script that creates particle emitters**, and 60 of the
/// generated scenery placements do.
///
/// The rest of a static object — its triangles — is already drawn by the baked static batches, so
/// this carries only what those batches cannot: the object identity the script hangs off.
///
/// **A placement whose setup names a default animation is one too**, and is not baked: static
/// initialization puts every static whose setup names a default animation or a default script on
/// one list, and every physics tick plays the animation on its part array. A butterfly over the
/// grass or a turning sign is drawn from its live part array, posed every frame, rather than from
/// the static batches, which could only hold its first pose.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmitterPlacement {
    /// The host's authored cell. A particle emitter registers one shadow in this cell, not
    /// the ordinary host mesh's set of overlapping cells.
    pub(crate) cell: CellId,
    /// The setup-record (`0x02……`) id.
    pub(crate) setup: DataId,
    /// Its placement frame, **block-local**, exactly as the dat gives it.
    pub(crate) frame: Frame,
    /// Which collision static is the *same placement*.
    ///
    /// Retail stores one physics-object pointer for each static placement; that pointer is
    /// simultaneously the part array whose setup creation queued the
    /// `default_script` and the body `add_obj_to_cell` put in the cell. This build makes
    /// the two halves in different modules, so the index is carried instead of implied — **by
    /// construction**, at the one place both are pushed, never by matching positions afterwards.
    pub(crate) slot: StaticSlot,
    /// The body [`dereth_world_data::env_cells::CellStaticObjects::init`] made for [`Self::slot`], once it has
    /// run. `None` until then, and `None` for ever for a placement that produced no body at all —
    /// the client's own null entry for that placement.
    pub(crate) body: Option<PhysHandle>,
    /// The placement's scale: generated scenery's own, 1 for an authored static.
    pub(crate) scale: f32,
    /// Whether the setup names a default animation, so the placement's parts are posed every
    /// frame and drawn from the host rather than baked.
    pub(crate) animated: bool,
    /// Whether the bake would have drawn the placement from the objects' look (another era's
    /// files); an animated placement's parts follow the same verdict.
    pub(crate) from_look: bool,
    /// The other cells an animated interior static is registered in, each of which draws it
    /// with its own objects; empty for every other placement.
    pub(crate) shadows: Vec<CellId>,
}

/// What setup creation starts on a placed static: its default script, its default animation,
/// both or neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StaticDefaults {
    /// The setup names a default script.
    pub(crate) script: bool,
    /// The setup names a default animation.
    pub(crate) animation: bool,
}

impl StaticDefaults {
    /// Whether the placement is a live object: one on the static-animating list.
    pub(crate) fn hosted(self) -> bool {
        self.script || self.animation
    }
}

/// Which defaults a placement's setup record names. A placement that is not a setup record (a
/// bare graphics object) names neither.
///
/// The default script id is zero for the 3,774 end-of-retail setups that have none; the 2,161
/// nonzero ids are what setup creation queues. A default animation is rarer: 7 of the 177 setups
/// the scenery records grow name one (butterflies among them), as do 22 of the 1,475 authored
/// outdoor statics' setups and 59 of the 3,580 interior ones'.
pub(crate) fn static_defaults(store: &RetailDatStore, id: DataId) -> StaticDefaults {
    if dereth_dat::divine_type(id) != Some(DbType::Setup) {
        return StaticDefaults::default();
    }
    let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
        return StaticDefaults::default();
    };
    dereth_assets::Setup::decode_payload_in(store.era_of(id), id, &bytes).map_or(
        StaticDefaults::default(),
        |s| StaticDefaults {
            script: s.default_script_id != DataId(0),
            animation: s.default_anim_id != DataId(0),
        },
    )
}

/// Which of a block's two collision-static lists a placement is in, and where.
///
/// Two lists because retail has two producers: interior-cell furniture, and land-scene processing
/// for outdoor statics and generated scenery. `world.rs` keeps
/// them apart (`BlockDraw::cell_statics` and `BlockDraw::land_statics`) because
/// `WorldScene::init_cell_statics` registers them in that order on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StaticSlot {
    /// An index into `BlockDraw::cell_statics`.
    Cell(usize),
    /// An index into `BlockDraw::land_statics`.
    Land(usize),
}

/// One placed static object that runs a script, and the emitters that script created.
///
/// The client has no such type: this represents a physics object, minus the parts the static
/// batches already bake and minus the physics body. `MotionDriver` is the whole of what is left.
pub(crate) struct EmitterHost {
    pub(crate) cell: CellId,
    pub(crate) driver: dereth_animation::MotionDriver,
    /// The placement in **absolute** world coordinates — see [`collect`] for why it is not the
    /// renderer's viewer-relative space.
    pub(crate) frame: Frame,
    /// The index into the owning `BlockDraw::emitters` this host was spawned
    /// from. The hosts are *not* index-parallel with the placements (a placement whose setup record
    /// will not load produces none), so the link has to be stored; it is how
    /// `WorldScene::init_cell_statics` can hand a body to a host that already exists.
    pub(crate) placement: usize,
    /// This placement's physics object, matching the per-placement pointer
    /// held by the client. `None` when the placement produced no collision body
    /// (a setup with no geometry, or a cell physics was never given) or before
    /// `WorldScene::init_cell_statics` has run for the block.
    pub(crate) body: Option<PhysHandle>,
    /// The setup's default sound table, which a `SoundTable` animation
    /// hook resolves its `sound_type` against. `MotionDriver::set_setup` reads `default_anim_id`,
    /// `default_mtable_id`, `default_phstable_id` and `default_script_id` and not this one, so it
    /// is carried here exactly as `ObjectEntry::sound_table` carries it for a server object. Only
    /// **4** of the 2,161 scripted setups name a table at all, so this is
    /// `None` for almost every host in the world.
    pub(crate) sound_table: Option<DataId>,
    /// Whether the setup names a default animation, which this host plays and draws.
    pub(crate) animated: bool,
    /// When the animation last advanced, on the frame clock; `None` before its first step.
    pub(crate) update_time: Option<f64>,
    /// An animated host's drawing half: one entry per part, each holding every degrade level.
    /// Shared with every other host on the same setup. `None` for a host whose parts the static
    /// batches draw.
    pub(crate) meshes: Option<std::sync::Arc<Vec<crate::world_scene::PartLevels>>>,
    /// The level each part draws this frame.
    pub(crate) part_levels: Vec<u32>,
    /// Each part's draw-position frame this frame, in the renderer's viewer-block-relative space.
    pub(crate) part_draw_pos: Vec<Frame>,
    /// Each part's viewer distance this frame, its depth-sort key.
    pub(crate) part_cypt: Vec<f32>,
}

/// The longest gap a static's animation advances over, in seconds; after a longer one (a hitch, a
/// return from elsewhere) the animation picks up where it was.
const STATIC_MAX_STEP: f64 = 2.0;
/// A gap under this is no time at all: the clock is restarted and nothing advances.
const STATIC_NO_TIME: f64 = 0.0002;

impl EmitterHost {
    /// Make the live object for one placement: its setup's part array with the setup's defaults
    /// started (the default animation playing, the default script queued), at the placement's
    /// frame and scale. `origin` is the block's south-west corner in absolute world coordinates.
    /// `None` when the setup will not load.
    pub(crate) fn spawn(
        assets: &std::sync::Arc<dyn dereth_animation::data::AnimAssets>,
        p: &EmitterPlacement,
        slot: usize,
        origin: (f32, f32),
    ) -> Option<Self> {
        let setup = assets.setup(p.setup)?;
        // The setup's default sound-table id, which `MotionDriver::set_setup` does not read.
        // Taken before `set_setup` moves the record.
        let sound_table = setup.default_sound_table;
        let mut driver = dereth_animation::MotionDriver::new(std::sync::Arc::clone(assets));
        // Static-object creation receives `(setup_id, 0, 0)`: object id 0 and **not** dynamic.
        // Setup creation queues the default script on the object's script manager; its hooks run
        // on the first script update. The default animation is the sequence's one animation.
        if !driver.set_setup(setup) {
            return None;
        }
        // Generated scenery is created at its own scale; an authored static at 1.
        if p.scale != 1.0 {
            driver.scale = p.scale;
            driver
                .part_array
                .set_scale_internal(Vec3::new(p.scale, p.scale, p.scale));
        }
        // The placement in **absolute** world coordinates. See [`collect`]: a particle born with
        // `is_parent_local == 0` keeps its birth frame for its whole life, and a permanent
        // emitter's whole life outlasts every landblock scroll.
        let frame = Frame::new(
            Vec3::new(
                p.frame.origin.x + origin.0,
                p.frame.origin.y + origin.1,
                p.frame.origin.z,
            ),
            p.frame.rotation,
        );
        // Physics placement would put it in a cell; the script layer only asks whether it is in
        // one, and a placed static always is.
        driver.env.in_cell = true;
        driver.env.position = dereth_primitives::Position::new(CellId(0), frame);
        // Place the part array through the internal part update: a placed object's parts are in
        // world space from the moment it is placed.
        driver.update_parts(&frame);
        Some(Self {
            cell: p.cell,
            driver,
            frame,
            placement: slot,
            body: p.body,
            sound_table,
            animated: p.animated,
            update_time: None,
            meshes: None,
            part_levels: Vec::new(),
            part_draw_pos: Vec::new(),
            part_cypt: Vec::new(),
        })
    }

    /// The animation half of a static's physics tick at frame time `now`: advance the default
    /// animation by the time since the last advance and re-place the parts.
    ///
    /// A gap too short to open the physics tick ([`dereth_physics::globals::tick_is_due`]) is left
    /// to accumulate into the next frame, so a static steps when the world does; a gap longer than
    /// [`STATIC_MAX_STEP`] restarts the clock without advancing. A host with no default animation
    /// only has its parts re-placed. Returns whether the animation advanced.
    pub(crate) fn animate(&mut self, now: f64) -> bool {
        use dereth_physics::MotionSource;
        let mut advanced = false;
        if self.animated {
            match self.update_time {
                None => self.update_time = Some(now),
                Some(last) => {
                    let dt = now - last;
                    if dt < STATIC_NO_TIME {
                        self.update_time = Some(now);
                    } else if dereth_physics::globals::tick_is_due(dt) {
                        if dt <= STATIC_MAX_STEP {
                            let _ = self.driver.advance(dt);
                            advanced = true;
                        }
                        self.update_time = Some(now);
                    }
                }
            }
        }
        // Place the parts **before** running scripts. A particle-creation hook names a part
        // index and captures that part's frame as the birth frame, which for an emitter with
        // `is_parent_local == 0` is where its particles live for ever.
        self.driver.update_parts(&self.frame);
        if advanced {
            // The hooks the animation frames just passed (a sound, a particle burst).
            self.driver.process_hooks();
        }
        advanced
    }
}

impl std::fmt::Debug for EmitterHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmitterHost")
            .field("origin", &self.frame.origin)
            .field("emitters", &self.driver.particles.len())
            .field("body", &self.body)
            .field("animated", &self.animated)
            .finish()
    }
}

/// One live particle, ready to draw: the emitter's mesh id and everything simulation wrote into
/// the part it owns.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParticlePart {
    /// The emitter's graphics-object id.
    pub(crate) gfx: DataId,
    /// The particle physics object's frame, already in the renderer's viewer-block-relative space.
    pub(crate) frame: Frame,
    /// The origin of the emitter's particle object, in the same space: the point the
    /// object-level viewer-distance update measures from. Every particle of one emitter carries
    /// the same one.
    pub(crate) object_origin: Vec3,
    /// **uniform**; there is no non-uniform particle scale.
    pub(crate) scale: f32,
    /// Part translucency: 1.0 means fully transparent and sets NoDraw.
    pub(crate) translucency: f32,
    /// Whether the emitter's cell is an outdoor one.
    ///
    /// Landscape drawing uses the sunlight set for outdoor objects, and
    /// cell drawing draws the rest with sunlight off, and mesh drawing runs
    /// `minimize_object_lighting` whenever sunlight is off. The same split is carried by
    /// `PartSubmission::outdoors`.
    pub(crate) outdoors: bool,
}

/// Drain one object's emitters into the draw list.
///
/// `shift` moves the emitter's **absolute** world coordinates into the renderer's
/// viewer-block-relative space. The simulation runs in absolute coordinates on purpose: a
/// particle born with `is_parent_local == 0` keeps the frame it was born in for its whole life
/// (particle updating chooses each particle's `start_frame`), and that frame is inside
/// `dereth_animation`'s `Particle`. Simulating in the viewer-relative space leaves every live
/// particle 192 metres behind the moment the landblock window scrolls unless the scroll moves it
/// too, which the re-centre does for the server objects and the body (they are simulated in the
/// renderer's space and passed a zero `shift`).
pub(crate) fn collect(
    mgr: &ParticleManager,
    shift: Vec3,
    outdoors: bool,
    out: &mut Vec<ParticlePart>,
) {
    for e in mgr.iter() {
        let gfx = e.info.hw_gfxobj_id;
        let object_origin = e.object_origin.add(shift);
        for p in e.live() {
            // The translucency convention: exactly 1.0 sets NoDraw, so a particle that fades to
            // `final_trans = 1.0` disappears cleanly on its last frame rather than drawing a
            // fully transparent quad.
            if p.translucency >= 1.0 {
                continue;
            }
            let mut frame = p.frame;
            frame.origin = frame.origin.add(shift);
            out.push(ParticlePart {
                gfx,
                frame,
                object_origin,
                scale: p.scale,
                translucency: p.translucency,
                outdoors,
            });
        }
    }
}

/// What one draw pass reported, for [`crate::world_scene::SceneStats`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParticleStats {
    /// Emitters alive across every object this frame.
    pub emitters: usize,
    /// Live particles across those emitters.
    pub live: usize,
    /// Particles that survived the particle draw gate, NoDraw and the LOD terminator and were drawn.
    pub drawn: usize,
    /// Draw calls the particles cost: one per surface batch per drawn particle.
    pub batches: usize,
    /// Distinct emitter meshes resolved.
    pub meshes: usize,
    /// Particles whose selected degrade level names no geometry; drawing requires a non-null
    /// graphics object at that level.
    pub degraded_out: usize,
    /// Particles whose `hw_gfxobj_id` produced no triangles. A non-zero count is an invisible
    /// effect and is asserted zero by `dereth/client/tests/gpu/rendering/particles.rs`.
    pub missing_geometry: usize,
}

/// One particle part after preparation: its sort key, mesh, and draw constants.
///
/// Not `Copy`, because it carries its own light set: `minimize_object_lighting` runs per part in the client and the answer is at most eight `D3DLIGHT9`s, so it is carried
/// rather than recomputed at submission time.
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    /// The distance to the scaled sort center and the key used to order a cell's parts.
    pub(crate) cypt: f32,
    /// The emitter mesh id.
    pub(crate) gfx: DataId,
    /// The `PerDraw` block: `draw_pos` (billboarded) scaled by `gfxobj_scale`.
    pub(crate) world: PerDrawConstants,
    /// `D3DRS_TEXTUREFACTOR`, carrying `(1 - translucency) * 255` in its alpha byte.
    pub(crate) texture_factor: u32,
    /// The `D3DLIGHT9`s this one particle is drawn with. Empty when
    /// `SceneConfig::object_lighting` is clear, which is the unlit control and leaves
    /// `D3DRS_LIGHTING` off exactly as the part path's `lights: None` does.
    pub(crate) lights: Vec<D3dLight>,
}

/// Compute viewer distance and choose a degrade level for every collected particle, then sort the
/// resulting parts far to near.
///
/// ```text
/// if the emitter's object is in a land cell more than 50 units away horizontally:
///     depth, viewer_heading = the cell's horizontal distance and heading
/// else if the emitter's object is at or past the particle share distance horizontally:
///     depth, viewer_heading = the object's own distance and heading
/// else:
///     c = gfxobj_scale (.) gfxobj[0]->sort_center
///     v = viewer_offset(part.pos, c)
///     depth = |v| ; viewer_heading = (depth <= 0.0002) ? (0,0,1) : v / depth
/// (degrades, depth / gfxobj_scale.z, &deg_level, &deg_mode)
/// if gfxobj[deg_level]: calc_draw_frame()
/// ```
///
/// `share` is the governor's current output: a particle emitter's object shares at its
/// **particle** distance, not at the object distance. Past it every particle of the emitter faces
/// along the one heading and sorts at the one distance.
///
/// The result is ordered by **descending viewer-space depth** — farthest first, which is what makes
/// overlapping translucent particles resolve.
pub(crate) fn prepare(
    geometry: &ParticleGeometry,
    viewer: Vec3,
    parts: &[ParticlePart],
    share: &DegradeLevel,
    globals: &DegradeGlobals,
    light_set: Option<ParticleLightSet<'_>>,
    stats: &mut ParticleStats,
) -> Vec<Prepared> {
    let mut ready: Vec<Prepared> = Vec::with_capacity(parts.len());
    for p in parts {
        let Some(gfx) = geometry.get(p.gfx) else {
            stats.missing_geometry += 1;
            continue;
        };
        let shared = dereth_world_render::objects::parts::object_viewer_distance(
            p.object_origin,
            p.outdoors,
            viewer,
            share.share_distance_2dsq(true),
        );
        let (cypt, heading) = shared.unwrap_or_else(|| {
            let c = gfx.sort_center.mul(p.scale);
            let v = p
                .frame
                .origin
                .add(localtoglobalvec(l2g(p.frame.rotation), c))
                .sub(viewer);
            let cypt = v.mag2().sqrt();
            let heading = if cypt <= 0.0002 {
                Vec3::new(0.0, 0.0, 1.0)
            } else {
                v.mul(1.0 / cypt)
            };
            (cypt, heading)
        });
        let mut mode = DegradeMode::None;
        if let Some(info) = gfx.degrade.as_ref() {
            // The divide by `gfxobj_scale.z` is the client's, and for a particle that scale is the
            // particle's own: a flame that starts at 0.1 and grows to 3.0 widens its own LOD band
            // as it grows, which is how a small particle is cut at short range and a large one is
            // not. This is the same divide used by viewer-distance selection.
            let (level, m) = get_degrade(info, cypt / p.scale.max(f32::MIN_POSITIVE), globals);
            // `gfxobj_id == 0` on the selected level means **draw nothing**,
            // and tests exactly that before it issues anything.
            // Every particle gfxobj in the retail dat ends in that all-`FLT_MAX` terminator, so it
            // is what stops a torch flame drawing past its band.
            if info.degrades.get(level).is_some_and(|d| d.gfxobj_id.0 == 0) {
                stats.degraded_out += 1;
                continue;
            }
            mode = m;
        }
        let draw_frame = calc_draw_frame(&p.frame, mode, heading);
        // The world transform uses `draw_pos` and the part's own `gfxobj_scale`.
        let mut world = world_constants_scaled(&draw_frame, Vec3::new(p.scale, p.scale, p.scale));
        // The translucency channel clones the material and
        // The material override writes `1 - t` into all four alpha
        // components; material binding then makes the **material** the source of the
        // diffuse colour instead of the vertex.
        // `legacy.hlsl`'s `arg2 = lerp(i.color, g_textureFactor, g_drawParams.w)` is that switch,
        // so a constant colour replaces the opaque white baked into the vertices.
        let alpha = dereth_primitives::num::to_i32((1.0 - p.translucency) * 255.0).clamp(0, 255);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: clamped to 0..=255 on the line above. Not a float conversion.
        let texture_factor = ((alpha as u32) << 24) | 0x00FF_FFFF;
        world.draw_params[3] = 1.0;
        // The light set `minimize_object_lighting` would choose for
        // this one card: the view-cone check scales the selected mesh's drawing sphere by
        // the part's `gfxobj_scale` and places it through `draw_pos`; object-light removal tests
        // every light against the result. The same arithmetic
        // `WorldScene::submission_light_set` runs for an animated part, on the particle's own
        // billboarded frame.
        let lights = light_set.map_or_else(Vec::new, |f| {
            let (centre, radius) = match gfx.drawing_sphere {
                Some((c, r)) => {
                    let s = p.scale;
                    let scaled = Vec3::new(c.x * s, c.y * s, c.z * s);
                    (
                        dereth_terrain::math::localtoglobal(&draw_frame, scaled),
                        s * r,
                    )
                }
                None => (draw_frame.origin, 0.0),
            };
            f(centre, radius, p.outdoors)
        });
        ready.push(Prepared {
            cypt,
            gfx: p.gfx,
            world,
            texture_factor,
            lights,
        });
    }
    // Sort by **descending viewer-space depth** — farthest first.
    // ORDER-OK: a total order on one f32 key with the emitter mesh id as the tie-break, so the
    // result does not depend on the collection order the way an unstable sort would.
    ready.sort_by(|a, b| {
        b.cypt
            .total_cmp(&a.cypt)
            .then_with(|| a.gfx.0.cmp(&b.gfx.0))
    });
    stats.drawn += ready.len();
    ready
}

/// Row 13 of the material-state table, the material-alpha override, is **not** re-derived here.
///
/// ```text
/// if (no current material || has_alpha == 0 || (alphaBlend && !alphaTest)) {
///     zwrite = !(alphaBlend && !alphaTest)
/// } else {
///     src = SRCALPHA; dst = INVSRCALPHA; alphaBlend = true; alphaTest = false; zwrite = false
/// }
/// ```
///
/// The translucency path sets `has_alpha` after checking the alpha values whenever
/// `1 - t < 1`, i.e. for every particle that is not fully opaque. An `Additive` surface already
/// satisfies `(alphaBlend && !alphaTest)` and so keeps its `ONE / ONE` — which is what makes glows
/// and magic effects look right.
///
/// **It comes from [`PartMesh::key_material_alpha`]**, which `dereth_scene::world_scene::resolve_surface`
/// gets from `PipelineKey::state_from_surface` with
/// `SurfaceContext::material_has_alpha = Some(true)` — the renderer's own table, so the particle
/// and animated-part paths share the same row of it.
/// Draw one subset of one prepared particle.
///
/// There is no particle renderer of its own: an emitter's particles are parts of their object,
/// sorted by viewer distance with every other part and put down through the ordinary per-subset
/// mesh path, in place or out of the alpha lists, which is where the object pass calls this from.
/// `force_alpha` is surface setup's force-alpha argument, set only for the alpha-list flush's draw
/// of an entry "Multiple Pass Alpha" queued.
///
/// # Errors
/// Any device failure from `Gpu::draw_dynamic`.
pub(crate) fn draw_one(
    gpu: &mut Gpu,
    per_frame: &PerFrameConstants,
    r: &Prepared,
    m: &crate::world_scene::PartMesh,
    lit: bool,
    force_alpha: bool,
    stats: &mut ParticleStats,
) -> Result<(), RenderError> {
    // The same keys every animated part picks between. A particle whose
    // `1 - t` is not 0xFF is a part whose material has alpha enabled.
    let key = *crate::world_scene::part_subset_key(m, r.texture_factor >> 24 != 0xFF, force_alpha);
    if let Some(slot) = m.texture {
        gpu.bind_texture(slot, m.sampler);
    }
    // Part drawing binds the material, then draws the mesh through the ordinary per-subset mesh
    // path. Two things follow:
    //
    // * **the lights.** Mesh drawing minimizes object lighting once per mesh against the
    //   drawing sphere just placed for this card. [`prepare`] computed that sphere and
    //   `bind_lights` applies the resulting set.
    // * **the emissive.** With `luminosity > 0`, subset drawing writes luminosity into the
    //   bound clone's `Emissive`, or into the default material when no clone is bound. At
    //   `<= 0`, it keeps the material's zero emissive. Lighting remains enabled for every subset.
    //
    // So a luminous particle still saturates to white and is the picture it always was, and a
    // `luminosity == 0` one is lit by the scene like any other part. **86 of the 268 distinct
    // surfaces the shipped particle emitters draw through are the latter**, referenced by 140
    // of the 2,051 shipped emitters.
    let mut world = r.world;
    if lit {
        world.material_lighting = PARTICLE_MATERIAL_LIGHTING;
        let emissive = if m.luminosity > 0.0 {
            m.luminosity
        } else {
            PARTICLE_MATERIAL_LIGHTING[0]
        };
        crate::world_scene::bind_lights(&mut world, &r.lights, emissive, false);
    }
    gpu.draw_dynamic(
        &key,
        &DrawConstants {
            alpha_ref: m.alpha_ref,
            texture_factor: r.texture_factor,
            ..DrawConstants::default()
        },
        per_frame,
        &world,
        &m.vertices,
    )?;
    stats.batches += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::motion::GfxObjInfo;
    use dereth_render::pso::Blend;

    fn info(levels: &[(u32, f32)]) -> GfxObjDegradeInfo {
        GfxObjDegradeInfo {
            id: DataId(0x1100_0001),
            degrades: levels
                .iter()
                .map(|&(id, d)| GfxObjInfo {
                    gfxobj_id: DataId(id),
                    degrade_mode: 5,
                    min_dist: d,
                    ideal_dist: d,
                    max_dist: d,
                })
                .collect(),
        }
    }

    fn entry(degrade: Option<GfxObjDegradeInfo>) -> ParticleGfx {
        ParticleGfx {
            meshes: Vec::new(),
            sort_center: Vec3::ZERO,
            degrade,
            drawing_sphere: None,
        }
    }

    /// ORACLE: the client's particle distance cut-off:
    /// `degrades[num_degrades-2].max_dist` (or `degrades[0].max_dist` with ≤ 2 levels), defaulting
    /// to 100.0 with no degrade info.
    #[test]
    fn get_max_degrade_distance_reads_the_second_to_last_level() {
        let mut g = ParticleGeometry::default();
        g.insert(DataId(1), Some(entry(None)));
        assert_eq!(g.max_degrade_distance(DataId(1)), 100.0, "no degrade info");
        // An id nobody resolved is the same case.
        assert_eq!(g.max_degrade_distance(DataId(9)), 100.0);

        // The shape every retail particle gfxobj has: the mesh, then an all-FLT_MAX terminator.
        g.insert(
            DataId(2),
            Some(entry(Some(info(&[(0x0100_1689, 64.0), (0, f32::MAX)])))),
        );
        assert_eq!(
            g.max_degrade_distance(DataId(2)),
            64.0,
            "two levels -> degrades[0]"
        );

        // Three levels take the second to last, not the last.
        g.insert(
            DataId(3),
            Some(entry(Some(info(&[(1, 10.0), (2, 40.0), (0, f32::MAX)])))),
        );
        assert_eq!(g.max_degrade_distance(DataId(3)), 40.0);
    }

    /// ORACLE: a degrade level with `gfxobj_id == 0` means **draw nothing**. Every particle gfxobj
    /// in the retail dat ends in that terminator, so this
    /// is what stops a torch flame being drawn beyond its band; the emitter's own
    /// `ShouldDrawParticles` cut-off is a second, coarser gate on the simulation side.
    #[test]
    fn the_terminator_level_selects_no_geometry() {
        let i = info(&[(0x0100_1689, 64.0), (0, f32::MAX)]);
        let g = DegradeGlobals::default();
        let (near, _) = get_degrade(&i, 10.0, &g);
        assert_eq!(near, 0);
        assert_ne!(
            i.degrades[near].gfxobj_id.0, 0,
            "inside the band there is geometry"
        );
        let (far, _) = get_degrade(&i, 500.0, &g);
        assert_eq!(far, 1);
        assert_eq!(
            i.degrades[far].gfxobj_id.0, 0,
            "outside it the level names nothing"
        );
    }
    fn part(gfx: u32, x: f32, scale: f32, translucency: f32) -> ParticlePart {
        ParticlePart {
            gfx: DataId(gfx),
            frame: Frame::new(Vec3::new(x, 0.0, 0.0), dereth_primitives::Quat::IDENTITY),
            object_origin: Vec3::new(x, 0.0, 0.0),
            scale,
            translucency,
            outdoors: false,
        }
    }

    /// ORACLE: viewer-distance selection sorts a cell's shadow part list **descending by
    /// viewer-space depth — farthest first. Particle parts use the same order because they *are*
    /// shadow parts.
    ///
    /// A near-to-far order would put a near particle's blend behind a far one, which is the
    /// classic translucency artefact this ordering exists to avoid.
    #[test]
    fn the_prepared_parts_come_out_far_to_near() {
        let mut g = ParticleGeometry::default();
        // No degrade record: every part survives, so the order is the only thing under test.
        g.insert(DataId(7), Some(entry(None)));
        let parts = vec![
            part(7, 10.0, 1.0, 0.0),
            part(7, 90.0, 1.0, 0.0),
            part(7, 50.0, 1.0, 0.0),
        ];
        let mut stats = ParticleStats::default();
        let ready = prepare(
            &g,
            Vec3::ZERO,
            &parts,
            &DegradeLevel::startup(),
            &DegradeGlobals::default(),
            None,
            &mut stats,
        );
        let keys: Vec<f32> = ready.iter().map(|r| r.cypt).collect();
        assert_eq!(keys, vec![90.0, 50.0, 10.0], "descending viewer distance");
        assert_eq!(stats.drawn, 3);
        assert_eq!(stats.degraded_out, 0);
        assert_eq!(stats.missing_geometry, 0);

        // A mesh nobody resolved is counted, not silently skipped: an invisible effect is a bug.
        let mut stats = ParticleStats::default();
        let _ = prepare(
            &g,
            Vec3::ZERO,
            &[part(8, 1.0, 1.0, 0.0)],
            &DegradeLevel::startup(),
            &DegradeGlobals::default(),
            None,
            &mut stats,
        );
        assert_eq!(stats.missing_geometry, 1);
    }

    /// Behaviour: rendering.particles.past-the-particle-share-distance-an-emitters-particles-take-its-distance-and-heading
    #[test]
    fn past_the_particle_share_distance_every_particle_takes_the_emitters_distance_and_heading() {
        let mut g = ParticleGeometry::default();
        // One upright-card level with a band far past any distance used here.
        g.insert(
            DataId(7),
            Some(entry(Some(info(&[(0x0100_0001, 1000.0), (0, f32::MAX)])))),
        );
        let origin = Vec3::new(20.0, 0.0, 0.0);
        let particle = |x: f32, y: f32, z: f32, translucency: f32| ParticlePart {
            gfx: DataId(7),
            frame: Frame::new(Vec3::new(x, y, z), dereth_primitives::Quat::IDENTITY),
            object_origin: origin,
            scale: 1.0,
            translucency,
            outdoors: false,
        };
        let parts = [
            particle(19.0, 3.0, 1.0, 0.0),
            particle(21.0, -3.0, 2.0, 0.5),
        ];
        // The client's startup outputs: objects share from 5 m, particle emitters from 4 m.
        let share = DegradeLevel::startup();
        let run = |viewer: Vec3| {
            let mut stats = ParticleStats::default();
            prepare(
                &g,
                viewer,
                &parts,
                &share,
                &DegradeGlobals::default(),
                None,
                &mut stats,
            )
        };
        let turned = |p: &ParticlePart, heading: Vec3| {
            world_constants_scaled(
                &calc_draw_frame(&p.frame, DegradeMode::AxisZ, heading),
                Vec3::new(1.0, 1.0, 1.0),
            )
            .world
        };
        let by_alpha = |ready: &[Prepared], t: f32| {
            let a = dereth_primitives::num::to_i32((1.0 - t) * 255.0);
            ready
                .iter()
                .find(|r| i64::from(r.texture_factor >> 24) == i64::from(a))
                .expect("the particle was prepared")
                .clone()
        };

        // 20 m out, past a 4 m share distance: both take the emitter's 20 m and its heading.
        let ready = run(Vec3::ZERO);
        for p in &parts {
            let r = by_alpha(&ready, p.translucency);
            assert!((r.cypt - 20.0).abs() < 1e-4, "{}", r.cypt);
            assert_eq!(r.world.world, turned(p, Vec3::new(1.0, 0.0, 0.0)));
        }

        // 4.5 m out across the ground: past the particle distance though inside the object one,
        // so the particles still share.
        let viewer = Vec3::new(15.5, 0.0, 0.0);
        let ready = run(viewer);
        for p in &parts {
            let r = by_alpha(&ready, p.translucency);
            assert!((r.cypt - 4.5).abs() < 1e-4, "{}", r.cypt);
            assert_eq!(r.world.world, turned(p, Vec3::new(1.0, 0.0, 0.0)));
        }

        // 1 m from the emitter horizontally, inside it: each is measured where it is.
        let viewer = Vec3::new(19.0, 0.0, 0.0);
        let ready = run(viewer);
        for p in &parts {
            let r = by_alpha(&ready, p.translucency);
            let v = p.frame.origin.sub(viewer);
            let own = v.mag2().sqrt();
            assert!((r.cypt - own).abs() < 1e-4, "{} against {own}", r.cypt);
            assert_eq!(r.world.world, turned(p, v.mul(1.0 / own)));
        }
    }

    /// ORACLE: translucency sets `alpha = 1 - t` on all four
    /// material colours, and the material-opacity rule is `curr_alpha = (int)((1 -
    /// translucency) * 255)`.
    ///
    /// The truncation is `dereth_primitives::num::to_i32`'s, not `as i32` — the lint forbids the raw cast and
    /// the client's own conversion truncates toward zero through a helper.
    #[test]
    fn the_translucency_ramp_reaches_the_draw_as_the_material_alpha() {
        let mut g = ParticleGeometry::default();
        g.insert(DataId(7), Some(entry(None)));
        let mut stats = ParticleStats::default();
        let parts = vec![
            part(7, 5.0, 1.0, 0.0),
            part(7, 6.0, 1.0, 0.5),
            part(7, 7.0, 1.0, 0.9),
        ];
        let ready = prepare(
            &g,
            Vec3::ZERO,
            &parts,
            &DegradeLevel::startup(),
            &DegradeGlobals::default(),
            None,
            &mut stats,
        );
        // Sorted far to near, so 0.9 is first.
        let alphas: Vec<u32> = ready.iter().map(|r| r.texture_factor >> 24).collect();
        assert_eq!(
            alphas,
            vec![25, 127, 255],
            "(1 - t) * 255, truncated toward zero"
        );
        // Every particle draw reads the texture factor rather than the vertex colour, which is
        // the material as the diffuse-colour source.
        assert!(ready.iter().all(|r| r.world.draw_params[3] == 1.0));
    }

    /// ORACLE: the translucency convention is **1.0 means fully transparent** (it sets NoDraw), and
    /// the `t == 1.0` arm writes `draw_state |= 1`.
    /// A particle at translucency 1.0 is **alive and not drawn**, which is not the same thing as
    /// dead: `KillParticle` runs off the lifespan, not off the ramp.
    #[test]
    fn a_fully_translucent_particle_is_alive_and_not_drawn() {
        use dereth_animation::data::{ParticleEmitterInfo, ParticleType};
        use dereth_animation::particles::{EmitterContext, ParticleManager};
        use dereth_primitives::num::rng::Ran2;
        use std::sync::Arc;

        let emitter = |start_trans: f32, final_trans: f32| {
            let mut i = ParticleEmitterInfo {
                particle_type: ParticleType::Still,
                hw_gfxobj_id: DataId(0x0100_1689),
                max_particles: 2,
                initial_particles: 2,
                lifespan: 10.0,
                start_trans,
                final_trans,
                ..ParticleEmitterInfo::default()
            };
            i.init_end();
            Arc::new(i)
        };
        let ctx = EmitterContext {
            parent_frame: Frame::default(),
            part_frame: None,
            emitter_origin: Vec3::ZERO,
            now: 0.0,
            should_draw: true,
        };
        let mut m = ParticleManager::new();
        let mut rng = Ran2::new(1);
        m.create_particle_emitter(
            emitter(0.0, 0.0),
            0xFFFF_FFFF,
            Frame::default(),
            1,
            &ctx,
            &mut rng,
        );
        m.create_particle_emitter(
            emitter(1.0, 1.0),
            0xFFFF_FFFF,
            Frame::default(),
            2,
            &ctx,
            &mut rng,
        );
        assert_eq!(
            m.iter().map(|e| e.live().count()).sum::<usize>(),
            4,
            "four live particles"
        );

        let mut out = Vec::new();
        collect(&m, Vec3::ZERO, false, &mut out);
        assert_eq!(
            out.len(),
            2,
            "only the opaque emitter's two reach the draw list"
        );
        assert!(out.iter().all(|p| p.translucency < 1.0));
    }

    /// The material alpha override spares an additive surface.
    #[test]
    fn the_material_alpha_override_spares_an_additive_surface() {
        use dereth_render::pso::{PipelineKey, SurfaceContext};
        use dereth_render::surface::{surface_type, Surface, SurfaceHandler};
        use dereth_render::vertex::VertexFormat;

        let ctx = SurfaceContext {
            vertex_format: VertexFormat::XyzDiffuseTex1,
            texture_is_set: true,
            lighting: false,
            ..SurfaceContext::default()
        };
        let of = |t: u32, material: Option<bool>| {
            let s = Surface {
                r#type: t,
                handler: SurfaceHandler::Database,
                ..Surface::default()
            };
            PipelineKey::state_from_surface(
                &s,
                SurfaceContext {
                    material_has_alpha: material,
                    ..ctx
                },
            )
            .key
        };

        // Additive: SRCALPHA / ONE, blended, no alpha test -> row 13's first branch, untouched.
        let ty = surface_type::BASE1_IMAGE | surface_type::ALPHA | surface_type::ADDITIVE;
        let add = of(ty, None);
        assert_eq!(add.dst_blend, Blend::One, "the additive destination factor");
        assert_eq!(of(ty, Some(true)), add, "additive keeps ONE");

        // A clip-mapped surface is alpha-tested, so the override does apply and takes the test off.
        let clip = of(
            surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP,
            None,
        );
        assert!(clip.alpha_test);
        let over = of(
            surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP,
            Some(true),
        );
        assert!(!over.alpha_test && over.alpha_blend && !over.z_write);
        assert_eq!(over.src_blend, Blend::SrcAlpha);
        assert_eq!(over.dst_blend, Blend::InvSrcAlpha);
        // An opaque particle never carries a material at all, so nothing is overridden.
        assert_eq!(
            of(
                surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP,
                Some(false)
            ),
            clip
        );
    }

    /// ORACLE: the alpha-list flush draws an entry "Multiple Pass Alpha" queued through surface
    /// setup with force alpha, which blends `SRCALPHA / INVSRCALPHA`, skips the alpha test and the
    /// depth write, and keeps the `LESS` depth test; the first, in-place pass is the alpha-tested
    /// cut-out. A particle's material alpha does not change the second pass.
    #[test]
    fn a_forced_particle_pass_is_blended_untested_and_writes_no_depth() {
        use dereth_render::pso::{PipelineKey, SurfaceContext, ZFunc};
        use dereth_render::surface::{surface_type, Surface, SurfaceHandler};
        use dereth_render::vertex::VertexFormat;

        let key = |t: u32, force_alpha: bool| {
            let s = Surface {
                r#type: t,
                handler: SurfaceHandler::Database,
                ..Surface::default()
            };
            let ctx = SurfaceContext {
                vertex_format: VertexFormat::XyzDiffuseTex1,
                texture_is_set: true,
                lighting: false,
                force_alpha,
                ..SurfaceContext::default()
            };
            PipelineKey::from_surface(&s, ctx).0
        };
        let mesh = |t: u32| crate::world_scene::PartMesh {
            key: key(t, false),
            surface_type: t,
            key_material_alpha: key(t, false),
            key_force_alpha: key(t, true),
            key_detail: key(t, false),
            alpha_ref: 0,
            texture: None,
            sampler: 0,
            subset_mask: dereth_world_render::objects::draw::subset_mask(t),
            surface: None,
            luminosity: 0.0,
            vertices: Vec::new(),
        };
        let clip_type = surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP;
        let clip = mesh(clip_type);
        let first = *crate::world_scene::part_subset_key(&clip, false, false);
        assert!(
            first.alpha_test && first.z_write,
            "the first pass is the cut-out"
        );
        for material_alpha in [false, true] {
            let second = *crate::world_scene::part_subset_key(&clip, material_alpha, true);
            assert!(second.alpha_blend, "the second pass blends");
            assert!(!second.alpha_test, "the second pass is not alpha-tested");
            assert!(!second.z_write, "the second pass writes no depth");
            assert_eq!(second.z_func, ZFunc::Less, "and still tests it with LESS");
            assert_eq!(second.src_blend, Blend::SrcAlpha);
            assert_eq!(second.dst_blend, Blend::InvSrcAlpha);
        }
    }
}
