//! Object identities, palettes and surface resolution.

use super::*;

impl ObjectLook {
    /// The look of `files` (an [`RetailDatStore::object_files`] store) under `identity`, with a
    /// surface cache set up as `like`, the world's, is.
    pub(super) fn new(
        files: RetailDatStore,
        interiors: Option<RetailDatStore>,
        identity: Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
        like: &BakeCache,
    ) -> Self {
        Self {
            files: Arc::new(files),
            interiors: interiors.map(Arc::new),
            cells: dereth_world_data::env_cells::EnvCellLoader::new(),
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
    pub(super) fn takes(&self, id: DataId, building: bool) -> bool {
        self.files.portal().contains(id)
            && self.identity.same(id)
            && (!building || self.identity.same_geometry(id))
    }
}

/// The verdicts for the world's portal against the other era's (`files`), and the world's
/// rooms against the other era's when its cell file is here (`interiors`), all at once: from
/// the host's cache folder when `cache` and it holds them, timed into the log.
pub(super) fn object_identity(
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
        RequiredFiles::Classic => dereth_dat::ContainerEra::Classic,
        RequiredFiles::Modern => dereth_dat::ContainerEra::Modern,
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
impl SceneDraw {
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
    pub(super) fn set_character_parts(&mut self, gpu: &mut Gpu, parts: Vec<PartLevels>) {
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
    pub(super) fn fold_object_counters(&mut self, c: world_objects::ObjectCounters) {
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
}

pub(super) fn read_surface(store: &RetailDatStore, id: DataId) -> Option<dereth_assets::Surface> {
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
pub(super) fn resolve_surface(
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
    let clip_map = decoded
        .as_ref()
        .is_some_and(|s| s.surface_type & dereth_render::surface::surface_type::BASE1_CLIPMAP != 0);
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
                    Some(w) if !texture_key.is_uncached() && !gpu.has_texture_key(texture_key) => {
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
        // draw, so the extra cost is one 1x1 texture per **distinct colour word**
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
pub(super) fn combined_key(
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
