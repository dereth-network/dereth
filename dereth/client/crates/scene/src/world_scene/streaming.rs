//! Scene construction, streaming, updates and resource release.

use super::*;

impl SceneDraw {
    /// The drawing half of a server object, which every object in
    /// `world.objects` has under the same id: the two maps gain and lose an id together.
    ///
    /// # Panics
    /// When `id` has no drawing half: something added it to `world.objects`, or removed its
    /// drawing half, outside [`Self::prepare_object_dispatch`], the one place that does both.
    pub(super) fn object_draw(&self, id: ObjectId) -> &SceneObject {
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
    pub(super) fn debug_check_object_halves(&self, ws: &WorldState) {
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
        let (sky_region, sky_store) = sky_for(store, &region, cfg.render.sky).unwrap_or_else(|e| {
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
                dereth_client_runtime::render_prefs::RenderPreferences::image_scale(
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
            cells: dereth_world_data::env_cells::EnvCellLoader::new(),
            static_geometry: HashMap::new(),
            cell_bsps: HashMap::new(),
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
            host_meshes: BTreeMap::new(),
            game_viewport: None,
            particle_gfx: ParticleGeometry::default(),
            frame_particles: std::cell::Cell::new(ParticleStats::default()),
            frame_material_parts: std::cell::Cell::new((0, 0)),
            frame_alpha_lists: std::cell::Cell::new(AlphaListStats::default()),
            frame_landscape_alpha: std::cell::Cell::new(LandscapeAlphaStats::default()),
            frame_alpha_pending: std::cell::RefCell::new(Vec::new()),
            frame_multipass_pending: std::cell::RefCell::new(Vec::new()),
            frame_alpha_order: std::cell::RefCell::new(Vec::new()),
            frame_static_blend: std::cell::RefCell::new(Vec::new()),
            frame_blend_order: std::cell::RefCell::new(Vec::new()),
            frame_object_cone: std::cell::Cell::new(ObjectConeStats::default()),
            frame_stamp: std::cell::Cell::new(0),
            frame_stamp_first: std::cell::Cell::new(0),
            frame_turn: std::cell::Cell::new(0),
            frame_cell_runs: std::cell::RefCell::new(Vec::new()),
            frame_cell_statics: std::cell::Cell::new(CellStaticDrawStats::default()),
            static_scratch: std::cell::RefCell::new(Vec::new()),
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

    /// The landscape window's bookkeeping — turn one window update into the
    /// work [`Self::stream`] has to do, release what left the window, and re-anchor what
    /// stayed.
    ///
    /// The re-anchor is the whole reason a scroll is cheap: the window
    /// gives every block an origin relative to the **viewer's** block, so a scroll changes
    /// every resident block's origin and nothing else. A block's vertices —
    /// terrain *and* objects — are block-local, so that is one `(f32, f32)` per block rather
    /// than a re-transform of everything in the window.
    pub(super) fn queue(&mut self, ws: &mut WorldState, actions: &[SlotAction]) {
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
    pub(super) fn release_block_interiors(&mut self, ws: &mut WorldState, departed: &[(i32, i32)]) {
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
    pub(super) fn release_block_terrain(&mut self, gpu: &mut Gpu, keys: &[MergeKey]) {
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
    pub(super) fn baked_texture_slots(b: &BakedObjects) -> (TextureLinks, TextureLinks) {
        Self::slots_of(&b.opaque, &b.blended, &b.env_cells)
    }

    /// A block's slots in two lists, its exterior objects' and its interior cells', each with
    /// whether it took its link from the objects' look's cache.
    pub(super) fn block_texture_slots(b: &BlockDraw) -> (TextureLinks, TextureLinks) {
        Self::slots_of(&b.opaque, &b.blended, &b.env_cells)
    }

    pub(super) fn slots_of(
        opaque: &[StaticBatch],
        blended: &[StaticBatch],
        env_cells: &[EnvCellDraw],
    ) -> (TextureLinks, TextureLinks) {
        fn batches(v: &[StaticBatch]) -> impl Iterator<Item = (TextureSlot, bool)> + '_ {
            v.iter().filter_map(|s| s.texture.map(|t| (t, s.from_look)))
        }
        let outside: Vec<(TextureSlot, bool)> = batches(opaque).chain(batches(blended)).collect();
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
    pub(super) fn release_look_texture(
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
    pub(super) fn release_departed_blocks(&mut self, gpu: &mut Gpu) {
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
        self.release_unlinked_host_meshes(gpu);
    }

    /// Drop the animated statics' geometry no host holds any more — the departed blocks' — and
    /// hand back the texture links its build took, one per mesh, as an object appearance's
    /// release does.
    pub(super) fn release_unlinked_host_meshes(&mut self, gpu: &mut Gpu) {
        let dead: Vec<(DataId, bool)> = self
            .host_meshes
            .iter()
            .filter(|(_, m)| Arc::strong_count(m) == 1)
            .map(|(k, _)| *k)
            .collect();
        for key in dead {
            let Some(meshes) = self.host_meshes.remove(&key) else {
                continue;
            };
            for part in meshes.iter() {
                for mesh in part.all() {
                    if let Some(slot) = mesh.texture {
                        self.release_look_texture(gpu, slot, part.from_look);
                    }
                }
            }
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
        // must never load (see [`dereth_world_data::land_source`]); "the same path that loads landblocks"
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
                dereth_client_runtime::render_prefs::RenderPreferences::image_scale(
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
    pub(super) fn load_sky(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
    ) -> Result<(), WorldError> {
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
    pub(super) fn rebuild_window(
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
        let (tex_merge, pal_shift) = land_surface(ground.as_ref().unwrap_or(&self.land.region))?;
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
    pub(super) fn objects_wait_for_identity(
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
                Some(RegionStyle::Late) => "with the later files' look",
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
    pub(super) fn apply_detail_texturing(&mut self, store: &RetailDatStore, gpu: &mut Gpu) {
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
            let Ok(data) = dereth_render::texture::scale_surface(&data, self.land.bake.image_scale)
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
    pub(super) fn detail_source(
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
        match style_region(store, RegionStyle::Late) {
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
    pub(super) fn release_detail_textures(&mut self, gpu: &mut Gpu) -> u32 {
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
    pub(super) fn current_detail(
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

    pub(super) fn flush_graphics_resources(&mut self, ws: &mut WorldState, gpu: &mut Gpu) {
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
    pub(super) fn init_cell_statics(&mut self, ws: &mut WorldState, store: &RetailDatStore) {
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
    pub(super) fn init_cell_restrictions(&mut self, ws: &mut WorldState) {
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
    pub(super) fn release_terrain_sources(&mut self, gpu: &mut Gpu) -> u32 {
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
    pub(super) fn convert_terrain(
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
    pub(super) fn release_unlinked_appearances(&mut self, gpu: &mut Gpu) -> (usize, u32) {
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
        let (center, radius) = parts
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
                    let subpixel =
                        dereth_primitives::num::to_i32_f64(f64::from(n) * 128.0 / f64::from(q.w));
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
    pub(super) fn worst_case_upload_bytes(&self) -> usize {
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
    /// `now` is current time, sampled once per frame by [`dereth_client_runtime::platform::clock::Timer`]. It reaches
    /// physics unchanged: the 30 Hz gate is inside the physics update and
    /// is **not** a frame-rate cap, so nothing here may pre-filter or quantise it.
    pub fn update(
        &mut self,
        ws: &mut WorldState,
        input: dereth_client_runtime::camera::CameraInput,
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
}
