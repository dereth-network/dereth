//! The sky — the game clock, and the sky objects over `dereth_world_render::sky`.
//!
//! **This module wires; it does not implement.** Everything that decides what the sky *is*
//! belongs to `dereth_world_render::sky` and is called from here by name:
//!
//! | Decision | Whose |
//! |---|---|
//! | the day group for today's date | `sky::present_day_group` / `sky::calc_present_day_group` |
//! | every object's mesh, heading, elevation and appearance at time `t` | `sky::get_sky` |
//! | the ambient level, the ambient colour and the sun vector | `sky::get_lighting` |
//! | the fog range and colour | `sky::get_world_fog` |
//! | the heading-then-rotation frame | `sky::calc_frame` |
//! | the two passes and pass 0's three skip rules | `sky::SkyPass`, `sky::draws_in_pass_0` |
//!
//! and the drawing is the ordinary object path: **the sky is not a shader and not a procedural
//! dome**, it is a handful of ordinary physics objects — a dome, two cloud layers, the sun, two
//! moons and a star field — living in two environment cells pinned to the viewer.
//!
//! # The game clock
//!
//! [`GameClock`], the game calendar, is not part of the sky: the sky takes `(year, day, t)` as
//! *given*, and the network layer only hard-sets the clock on a TimeSync. The calendar arithmetic
//! lives in [`dereth_client_runtime::game_clock`], because the world state owns the clock; this
//! module re-exports it.
//!
//! The implementation preserves the retail time, sky/environment, and lighting behaviour.

/// The calendar clock, re-exported from [`dereth_client_runtime::game_clock`]: it is the
/// simulation's, and the world state owns it.
pub use dereth_client_runtime::game_clock::GameClock;

#[cfg(gpu)]
pub use imp::*;

#[cfg(gpu)]
mod imp {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use dereth_assets::Region;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{DataId, Frame, Vec3};
    use dereth_render::device::{Gpu, PerFrameConstants};
    use dereth_render::{DrawConstants, RenderError, ViewParams, ZFunc};
    use dereth_world_render::sky::{self, SkyPass, SKY_ZFAR_MULTIPLIER};

    use crate::textures::TextureStore;
    use {
        crate::world_scene::build_meshes, crate::world_scene::world_constants,
        crate::world_scene::BakeCache, crate::world_scene::PartMesh,
    };

    /// The absolute height a weather object's origin is dropped to, in the viewer's landblock
    /// space — i.e. the crate's render space, whose `z` is the world height.
    ///
    /// Sky positioning leaves non-weather objects and bit-3 weather objects at their existing
    /// height. A weather object with bit 3 clear instead receives `-120.0` as the `z` coordinate
    /// of its local frame origin.
    ///
    /// **Skipping this store** (and the frame copy before it) in the weather branch would leave
    /// the weather layer at the viewer's eye. The shipped rain curtain's own geometry runs
    /// `z ∈ [0.1, 814.9]` with no part below its origin, so at the eye it could only ever be
    /// drawn *above* the horizon.
    ///
    /// This also explains bit 3: it means *do not drop this weather object to the floor*.
    const WEATHER_FLOOR_Z: f32 = -120.0;

    /// The sky objects and the two cells they live in.
    ///
    /// The two env cells are not modelled as cells: they exist in the client so that the sky
    /// participates in the ordinary cell draw and so that the per-frame position update can move both at once,
    /// and both facts reduce here to "which pass an object is in" and "where the viewer is".
    ///
    /// One entry of the texture-velocity table: a rate and its running total.
    #[derive(Debug, Clone, Copy, Default)]
    struct TexVelocityDesc {
        /// The per-second rate the day group named.
        offset: (f32, f32),
        /// The running sum handed to the mesh UV animation.
        total: (f32, f32),
    }

    #[derive(Debug)]
    pub struct SkyScene {
        /// Geometry per graphics id. The client destroys and recreates a sky object whenever its
        /// graphics id changes; this rebuild caches the geometry by id instead. Every id any day
        /// group can name is built once at load because
        /// `Gpu::upload_texture` runs a command list of its own and must not happen in a frame.
        // ORDER-OK: a memo keyed by DataId, only ever looked up.
        meshes: BTreeMap<DataId, Arc<Vec<PartMesh>>>,
        /// This frame's objects in the order returned by the day-group sky lookup.
        objects: Vec<SkyObject>,
        /// the accumulated texture velocity per **gfx id**,
        /// not per object, so two decks sharing a mesh scroll together as they do in the client.
        // ORDER-OK: a memo keyed by DataId, only ever looked up.
        tex_totals: BTreeMap<DataId, TexVelocityDesc>,
        /// Whether weather objects may be created; enabled by default.
        pub weather_enabled: bool,
        /// Whether the visual environment override is active. The pass-selection
        /// rules and the temporary fixed-function fog state both read this same value.
        pub override_enabled: bool,
        /// Regional fog state used by pass 0's skip predicate.
        pub fog_enabled: bool,
        pub stats: SkyStats,
    }

    /// One live sky object: sky object `[i]` plus its properties.
    #[derive(Debug)]
    struct SkyObject {
        properties: u32,
        /// The object's frame after heading and rotation are applied. The origin is `(0, 0, 0)` for
        /// everything except a weather object, which the per-frame position update pins to the viewer.
        frame: Frame,
        /// Drawing-state bit 1, derived from `cp.transparent * 0.01`
        /// — **1.0 means fully invisible and sets NoDraw rather than storing a value**.
        /// Two of the shipped sky objects are hidden
        /// this way at some times of day, so it is the difference between a cloud deck and none.
        no_draw: bool,
        /// The material override left by the three appearance channels on every
        /// part of this object, or `None` when all three use their defaults and no clone is needed.
        ///
        /// This is what makes night night. The luminosity channel writes
        /// emissive RGB, while diffuse setup writes
        /// `Diffuse.rgb`, while translucency writes `1 - t`
        /// into all four alphas; material construction leaves `Ambient` at
        /// (1, 1, 1, 1). With fixed-function lighting enabled for every subset, the D3D9 vertex
        /// pipeline then evaluates
        /// `Emissive + Ambient * D3DRS_AMBIENT + Diffuse * sum(lights)` — so the region's
        /// `luminosity` ramp *is* the sky's brightness, and an object whose ramp holds it at 0
        /// (the shipped night dome, index 1 of every group) draws as `D3DRS_AMBIENT` alone, which
        /// at Foredawn is the expected blue-purple ambient color.
        ///
        /// Dropping these three channels, keeping only the `transparent == 1.0` NoDraw, draws
        /// the cloud deck at full texture brightness at every hour: a white sheet across the
        /// night sky.
        material: Option<dereth_animation::parts::MaterialOverride>,
        /// The accumulated mesh UV delta for this object's `tex_velocity`.
        uv_offset: (f32, f32),
        meshes: Arc<Vec<PartMesh>>,
    }

    /// What the sky loaded and what it costs, for the log line and the acceptance test.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct SkyStats {
        /// Distinct graphics-object and setup ids any day group can name, all built at load.
        pub gfx_ids: usize,
        /// Of those, how many produced drawable triangles. A shortfall is a decode gap.
        pub gfx_ids_drawable: usize,
        /// Draws one full sky costs: every object's every surface group.
        pub batches: usize,
        pub triangles: usize,
        /// Objects present at the current time of day, i.e. sky-query entries with a live gfx id.
        pub live_objects: usize,
        /// Of those, how many pass 0 submits and how many pass 1 submits.
        pub pass0_objects: usize,
        pub pass1_objects: usize,
        /// Sky-query entries whose gfx id is set but whose geometry is not in the cache. Must be 0:
        /// a non-zero count means [`SkyScene::load`] missed an id the data can ask for, which is
        /// exactly the shape of failure a tolerant lookup hides.
        pub missing_geometry: u64,
    }

    impl SkyScene {
        /// once per frame with the frame's elapsed time,
        /// over **every** registered gfx id whether or not it is drawn this frame:
        ///
        /// ```text
        /// d = dt * offset;  if (total >= 1.0) d -= 1.0;  total += d
        /// ```
        ///
        /// The wrap tests the **old** total and subtracts a whole 1.0 from the *step*, which is the
        /// client's arithmetic and not the `fract()` anyone would reach for instead: the total sits
        /// a little above 1.0 for one frame before snapping back near zero.
        fn advance_tex_velocity(&mut self, dt: f32) {
            for d in self.tex_totals.values_mut() {
                let mut du = dt * d.offset.0;
                let mut dv = dt * d.offset.1;
                if d.total.0 >= 1.0 {
                    du -= 1.0;
                }
                if d.total.1 >= 1.0 {
                    dv -= 1.0;
                }
                d.total.0 += du;
                d.total.1 += dv;
            }
        }

        /// This id's accumulated total, registering it on first sight. A new entry starts at zero
        /// and is advanced by the next frame, exactly as the client's per-id velocity table does.
        fn tex_velocity_total(&mut self, id: DataId, offset: (f32, f32)) -> (f32, f32) {
            self.tex_totals
                .entry(id)
                .or_insert(TexVelocityDesc {
                    offset,
                    total: (0.0, 0.0),
                })
                .total
        }

        /// Build every sky object's geometry, for every day group the region carries.
        ///
        /// # Errors
        /// [`RenderError`] when a device resource cannot be created.
        pub(crate) fn load(
            store: &RetailDatStore,
            region: &Region,
            gpu: &mut Gpu,
            cache: &mut BakeCache,
        ) -> Result<Self, RenderError> {
            let textures = TextureStore::new(store);
            let mut meshes: BTreeMap<DataId, Arc<Vec<PartMesh>>> = BTreeMap::new();
            let mut stats = SkyStats::default();
            for id in gfx_ids(region) {
                // Sky object creation accepts a setup record (0x02) or a bare graphics object
                // (0x01);
                // the shipped sky uses both -- the star fields are setups with a particle emitter.
                let mut built: Vec<PartMesh> = Vec::new();
                for part in dereth_client_runtime::models::resolve_parts(store, id) {
                    let groups = dereth_client_runtime::models::build_gfxobj(store, part.gfxobj);
                    // A sky object never animates its parts, so the placement frame is folded into
                    // the vertices and the only matrix left is the object's own.
                    let mut m = build_meshes(
                        store,
                        cache,
                        &textures,
                        gpu,
                        &groups,
                        None,
                        Some(&part.frame),
                    )?;
                    // Sky drawing sets depth testing to always-pass with depth writes off for
                    // the whole of both passes: the sky never occludes anything and is never
                    // occluded. Baked into the pipeline key here because these meshes are the sky's
                    // alone -- the shared surface cache hands out the surface's own state and this
                    // is the one caller that overrides it.
                    //
                    // Two more things belong to that bracket. Sky drawing also sets
                    // the fixed-function fog enable: ordinary regional fog stays
                    // off for the sky, while an active Admin override temporarily enables it for
                    // both passes. The per-draw choice is applied in [`SkyScene::draw`], because
                    // the override can change after these cached meshes are built. The
                    // material-alpha key variant needs the same depth state because a sky object with a partial
                    // `SkyObjectReplace::transparent` is drawn through it.
                    for mesh in &mut m {
                        mesh.key.z_func = ZFunc::Always;
                        mesh.key.z_write = false;
                        mesh.key.fog = false;
                        mesh.key_material_alpha.z_func = ZFunc::Always;
                        mesh.key_material_alpha.z_write = false;
                        mesh.key_material_alpha.fog = false;
                    }
                    built.append(&mut m);
                }
                stats.gfx_ids += 1;
                if !built.is_empty() {
                    stats.gfx_ids_drawable += 1;
                }
                stats.batches += built.len();
                stats.triangles += built
                    .iter()
                    .map(|m| m.vertices.len() / (LAND_VERTEX_STRIDE * 3))
                    .sum::<usize>();
                meshes.insert(id, Arc::new(built));
            }
            Ok(Self {
                tex_totals: BTreeMap::new(),
                meshes,
                objects: Vec::new(),
                // Weather is enabled by default.
                weather_enabled: true,
                override_enabled: false,
                fog_enabled: false,
                stats,
            })
        }

        /// Re-hash the day group, ask `sky::get_sky` for every object's
        /// position at `t`, and reconcile the live object array with it.
        ///
        /// The luminosity / diffusion / translucency settings all land on the material — see
        /// `SkyObject::material` — because the shader carries the fixed-function vertex lighting
        /// and the material's Emissive and Diffuse. The texture-velocity UV delta is the `uv_offset` below.
        ///
        /// `asked` is the kind of day this client was asked to draw (CD-039), which takes the
        /// place of the calendar's when the day is not already of that kind.
        pub fn use_time(
            &mut self,
            region: &Region,
            (year, day): (u32, u32),
            asked: Option<sky::DayKind>,
            t: f32,
            dt: f32,
        ) {
            self.advance_tex_velocity(dt);
            let Some(group) = sky::drawn_day_group(region, year, day, asked) else {
                self.objects.clear();
                return;
            };
            let positions = sky::get_sky(group, t);
            // Nothing to carry across the rebuild: the UV offset is a constant per object, not an
            // accumulator, so re-deriving it from `tex_velocity` below reproduces it exactly. See
            // the note at the assignment.
            self.objects.clear();
            self.stats.live_objects = 0;
            self.stats.pass0_objects = 0;
            self.stats.pass1_objects = 0;
            // Weather-object creation reads the first sky object's frame origin rather than the
            // viewer's. Whether object 0 is guaranteed to be the viewer-locked dome is not known;
            // in the shipped data object 0's origin is always the identity's,
            // so the two readings agree.
            let mut first_origin = Vec3::ZERO;
            for (i, cp) in positions.iter().enumerate() {
                // Object creation: no object at all for a zero gfx id, or for a weather
                // object while weather is disabled.
                if cp.gfx_id.0 == 0 || (cp.properties & 4 != 0 && !self.weather_enabled) {
                    continue;
                }
                // Cloned so the immutable borrow of `meshes` ends here: the UV total below needs
                // `&mut self`, and an `Arc` clone is a refcount bump, not a copy of the geometry.
                let Some(meshes) = self.meshes.get(&cp.gfx_id).cloned() else {
                    self.stats.missing_geometry += 1;
                    continue;
                };
                let mut frame = Frame::default();
                if cp.properties & 4 != 0 {
                    frame.origin = first_origin;
                }
                sky::calc_frame(&mut frame, cp.heading, cp.rotation);
                if i == 0 {
                    first_origin = frame.origin;
                }
                self.stats.live_objects += 1;
                if sky::draws_in_pass_0(
                    cp.properties,
                    self.weather_enabled,
                    self.override_enabled,
                    self.fog_enabled,
                ) {
                    self.stats.pass0_objects += 1;
                } else if cp.properties & 1 != 0 {
                    self.stats.pass1_objects += 1;
                }
                // The sky update's three appearance calls, all three of them:
                //
                // a positive `luminosity` sets luminosity to `luminosity * 0.01`, a positive
                // `max_bright` sets diffusion to `max_bright * 0.01`, and a non-negative
                // `transparent` sets translucency to `transparent * 0.01`.
                //
                // The luminosity and diffusion settings are timed ramps with `duration = 0`, meaning "set the
                // end value now". Translucency **1.0 means fully invisible** and sets `NoDraw`
                // rather than storing a value, which two
                // shipped sky objects rely on to disappear at certain times of day; below 1.0 it
                // reaches the material copy like the other two.
                //
                // **Deviation, named.** In the client the physics object persists, so a frame in
                // which the sky query leaves a channel "unset" (-1) keeps the value the last frame put
                // there; here the objects are rebuilt every `use_time` and the value is
                // re-derived. Over the shipped region the two agree: an object that carries a
                // `SkyObjectReplace` for a channel carries one in **every** `sky_time` row of its
                // group, and one that carries none never has the channel written at all.
                let lum = if cp.luminosity > 0.0 {
                    cp.luminosity * 0.01
                } else {
                    dereth_animation::parts::DEFAULT_LUMINOSITY
                };
                let diffuse = if cp.max_bright > 0.0 {
                    cp.max_bright * 0.01
                } else {
                    dereth_animation::parts::DEFAULT_DIFFUSE
                };
                let translucency = if cp.transparent >= 0.0 {
                    cp.transparent * 0.01
                } else {
                    dereth_animation::parts::DEFAULT_TRANSLUCENCY
                };
                let no_draw = translucency == 1.0;
                // With all three channels at their defaults there is no clone at all, and
                // current-material selection takes Diffuse and Ambient from the vertex.
                let defaulted = translucency == dereth_animation::parts::DEFAULT_TRANSLUCENCY
                    && diffuse == dereth_animation::parts::DEFAULT_DIFFUSE
                    && lum == dereth_animation::parts::DEFAULT_LUMINOSITY;
                let material = (!defaulted).then_some(dereth_animation::parts::MaterialOverride {
                    translucency,
                    diffuse,
                    luminosity: lum,
                });
                // The texture-velocity update turns on the mesh's UV animation and sets its UV
                // delta. That is how the cloud decks drift and the rain falls.
                //
                // The update runs once per frame with the frame's elapsed time. It is **not** an increment per
                // frame and **not** a constant — both of those were tried here and both were wrong:
                //
                // ```text
                // d = dt * offset.uv;
                // if (total.uv >= 1.0) d -= 1.0;      // the wrap
                // total.uv += d;
                // apply_texture_velocity(gfxobj, total) // -> the mesh's UV delta
                // ```
                //
                // So the running total is what reaches the mesh's UV delta, and the mesh-subset
                // draw writes that straight into the `D3DTS_TEXTURE0`
                // translation — absolute at the point of use, accumulated before it. The client
                // keeps one total per **gfxobj id** in the global `texture_velocity_gids`, not one
                // per object, which is why two sky decks sharing a mesh scroll together.
                //
                // History, because both failures were instructive: accumulating one `tex_velocity`
                // per frame with no `dt` made the sky stream past at frame rate; removing the
                // accumulation stopped it dead. A person watching it caught each in turn.
                let uv_offset = self.tex_velocity_total(cp.gfx_id, cp.tex_velocity);
                self.objects.push(SkyObject {
                    properties: cp.properties,
                    frame,
                    no_draw,
                    material,
                    uv_offset,
                    meshes,
                });
            }
        }

        /// The `ViewParams` the sky is drawn with: the world's, with **`zfar` multiplied by 4**.
        ///
        /// Multiplying the render distance by 4.0 rebuilds the projection for the
        /// duration of both passes, so sky geometry can sit four times farther out than the world
        /// (16000 against the default 4000). The dome is 1050 m across and the cloud layers are
        /// 20 km, so this is not decoration: without it the layers are clipped away.
        #[must_use]
        pub fn view_params(world: &ViewParams) -> ViewParams {
            ViewParams {
                zfar: world.zfar * SKY_ZFAR_MULTIPLIER,
                ..*world
            }
        }

        /// Draw this sky pass.
        ///
        /// `viewer` is the viewer's origin in the render space, which is what
        /// sky positioning pins the two cells and the weather objects to. In
        /// full, and in the order retail applies them:
        ///
        /// ```text
        /// f = object.frame
        /// if props & 4: f.origin = viewer.origin
        /// else:         interpolate_origin(f, viewer.frame, 0.0) // no-op at t = 0
        /// if props & 4 and props & 8 == 0: f.origin.z = -120.0
        /// object.set_frame(f)
        /// ```
        ///
        /// So a **non-weather** object keeps the origin the sky update gave it — `(0, 0, 0)` in the
        /// viewer's own cell, i.e. the south-west corner of the viewer's landblock, which is the
        /// origin of this crate's render space by construction. It is effectively infinitely far
        /// away and only rotates. A **weather** object follows the viewer in `x` and `y` and sits
        /// at the fixed absolute height [`WEATHER_FLOOR_Z`], unless its bit 3 is set, in which
        /// case it takes the viewer's `z` as well. That third case is the two rainy groups' star
        /// field (`properties == 13`), which has no geometry of its own.
        ///
        /// `outside` is the player's outside-state query: indoors, only pass 0 runs.
        ///
        /// # Errors
        /// Any failure from the runtime.
        /// `lights` is the light set the sky is drawn under: the outdoor draw selects the sunlight-only light set and brackets its landscape
        /// pass with the two sky passes, so both sky passes run under the sun-only set — the
        /// same set the outdoor blocks and their objects take.
        pub fn draw(
            &self,
            gpu: &mut Gpu,
            pass: SkyPass,
            per_frame: &PerFrameConstants,
            viewer: Vec3,
            outside: bool,
            lights: &[dereth_world_render::lighting::D3dLight],
        ) -> Result<(), RenderError> {
            if !outside && pass != SkyPass::Before {
                return Ok(());
            }
            if pass == SkyPass::After && !self.weather_enabled {
                // Otherwise retail draws the after-sky cell's objects.
                return Ok(());
            }
            #[cfg(feature = "hifi")]
            let mut index = 0u32;
            for o in &self.objects {
                #[cfg(feature = "hifi")]
                let this = {
                    index += 1;
                    index - 1
                };
                let wanted = match pass {
                    SkyPass::Before => sky::draws_in_pass_0(
                        o.properties,
                        self.weather_enabled,
                        self.override_enabled,
                        self.fog_enabled,
                    ),
                    // Pass 1 is the whole `after_sky_cell`, which is every object with bit 0 set.
                    SkyPass::After => o.properties & 1 != 0,
                };
                // Drawing skips a part whose `draw_state` bit 0 is set.
                if !wanted || o.no_draw {
                    continue;
                }
                #[cfg(feature = "hifi")]
                gpu.hifi_mark(dereth_render::hifi_mark::Mark::SkyObject {
                    index: this,
                    properties: o.properties,
                });
                let origin = if o.properties & 4 == 0 {
                    o.frame.origin
                } else if o.properties & 8 == 0 {
                    // The weather layer is pinned to the viewer horizontally and to a **fixed
                    // absolute height** vertically. See [`WEATHER_FLOOR_Z`].
                    Vec3::new(viewer.x, viewer.y, WEATHER_FLOOR_Z)
                } else {
                    viewer
                };
                let base = world_constants(&Frame::new(origin, o.frame.rotation));
                // Object drawing opens by binding the object's own material, so
                // every one of this object's
                // subsets is drawn under the clone `use_time` left on it. The three channels are
                // exactly the ones `dereth_scene::world_scene` already binds for a body part: the texture
                // factor carries `1 - translucency` as the diffuse source
                // (diffuse colour sourced from the material, `draw_params.w`), and
                // `material_lighting` carries `Emissive` and `Diffuse`.
                let factor = crate::world_scene::material_texture_factor(o.material);
                let lighting = crate::world_scene::material_lighting(o.material);
                for m in o.meshes.iter() {
                    let mut world = base;
                    world.material_lighting = lighting;
                    if factor.is_some() {
                        world.draw_params[3] = 1.0;
                    }
                    // The mesh-subset draw resolves the Emissive it hands the device:
                    // the surface's own `luminosity` when positive, otherwise the bound clone's.
                    let emissive = if m.luminosity > 0.0 {
                        m.luminosity
                    } else {
                        lighting[0]
                    };
                    crate::world_scene::bind_lights(&mut world, lights, emissive, false);
                    if let Some(slot) = m.texture {
                        gpu.bind_texture(slot, m.sampler);
                    }
                    let mut key = if factor.is_some() {
                        m.key_material_alpha
                    } else {
                        m.key
                    };
                    // Sky drawing temporarily sets the fixed-function fog enable around both passes
                    // and restores the prior device state afterward. Keep every cached
                    // surface/material choice and change only that live fog bit.
                    key.fog = self.override_enabled;
                    gpu.draw_dynamic(
                        &key,
                        &DrawConstants {
                            alpha_ref: m.alpha_ref,
                            texture_factor: factor.unwrap_or(0),
                            uv_offset_bits: [o.uv_offset.0.to_bits(), o.uv_offset.1.to_bits()],
                            ..DrawConstants::default()
                        },
                        per_frame,
                        &world,
                        &m.vertices,
                    )?;
                }
            }
            Ok(())
        }

        /// The bytes one frame's sky pushes through the dynamic upload arena. Both passes together,
        /// because the reservation is per frame.
        #[must_use]
        pub fn upload_bytes(&self) -> usize {
            const CB: usize = 256; // D3D12_CONSTANT_BUFFER_DATA_PLACEMENT_ALIGNMENT
            self.objects
                .iter()
                .flat_map(|o| o.meshes.iter())
                .map(|m| m.vertices.len().div_ceil(CB) * CB + 2 * CB)
                .sum()
        }

        /// How many objects the sky is currently drawing, for the log line and the tests.
        #[must_use]
        pub fn live(&self) -> usize {
            self.objects.len()
        }
    }

    /// [`crate::world_scene::OBJECT_VERTEX_STRIDE`], the FVF `0x152` layout every graphics-object
    /// mesh in this crate is built in (the sky's included: `build_meshes` is one bake).
    const LAND_VERTEX_STRIDE: usize = crate::world_scene::OBJECT_VERTEX_STRIDE;

    /// Every graphics-object or setup id any day group of this region can ask for: each `SkyObject`'s
    /// `default_gfx_object`, plus every `SkyObjectReplace::gfx_obj_id` that can override one.
    fn gfx_ids(region: &Region) -> BTreeSet<DataId> {
        let mut out = BTreeSet::new();
        let Some(sky) = region.sky_info.as_ref() else {
            return out;
        };
        for g in &sky.day_groups {
            for o in &g.sky_objects {
                if o.default_gfx_object.0 != 0 {
                    out.insert(o.default_gfx_object);
                }
            }
            for t in &g.sky_time {
                for r in &t.sky_obj_replace {
                    if r.gfx_obj_id.0 != 0 {
                        out.insert(r.gfx_obj_id);
                    }
                }
            }
        }
        out
    }
}
