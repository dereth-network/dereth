//! Animation hooks and particle emitter lifecycle.

use super::*;

impl SceneDraw {
    /// Give every scripted placement a live object, and every emitter a mesh.
    ///
    /// Called from [`Self::sync_objects`] because that is the frame step handed both the
    /// shared `Arc<RetailDatStore>` a `MotionDriver` needs and the device the meshes need, and
    /// because `Gpu::upload_texture` runs a command list of its own and so may not be called
    /// inside the frame bracket.
    ///
    /// # Errors
    /// [`WorldError::Render`] when a device resource cannot be created.
    pub(super) fn bring_up_particles(
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
    pub(super) fn drain_host_hooks(
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
    pub(super) fn update_particles(
        &mut self,
        ws: &mut WorldState,
        now: dereth_primitives::LocalTime,
    ) {
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
    pub(super) fn mesh_of<'w>(
        &self,
        ws: &'w WorldState,
        block: (i32, i32),
    ) -> Option<&'w LandblockMesh> {
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
    pub fn replace_player_particle_script(&mut self, ws: &mut WorldState, script: DataId) -> bool {
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
    pub(super) fn block_shift(&self, ws: &WorldState) -> Vec3 {
        dereth_client_runtime::landblock::block_shift(
            ws.streamer.window.viewer_block(),
            self.cfg.landblock,
        )
    }

    /// Live particles owned by the requested pass-local cells.
    pub(super) fn collect_particles(
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
                crate::particles::collect(&c.driver().particles, Vec3::ZERO, outdoors, &mut out);
            }
        }
        out
    }

    /// Copy the shared surface cache's counters into [`SceneStats`].
    ///
    /// They are counters on *tolerant* paths — a sub-palette range that would not apply, a
    /// palette that is not in the dat — and this project has twice paid for a counter nothing
    /// compared, so `dereth/client/tests/gpu/objects/appearance_objdesc.rs` asserts both are zero over the whole corpus.
    pub(super) fn refresh_surface_stats(&mut self) {
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
}
