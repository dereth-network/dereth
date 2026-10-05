//! Part meshes, material state and vertex submission.

use super::*;

impl dereth_primitives::RenderBackend for TextureUploader<'_> {
    fn upload_mesh(&mut self, _m: &dereth_primitives::MeshData) -> dereth_primitives::MeshHandle {
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
impl SceneDraw {
    /// [`Self::build_part_meshes`] for a server object or the body, with the objects' look.
    ///
    /// Each part is built wholly from one era
    /// ([`dereth_client_runtime::models::parts_for_look`]): from the look's files through the
    /// look's surface cache, with its texture changes carried onto the look's pictures, when
    /// the look may stand for it; from `world` otherwise, and every part when the look is the
    /// world's own. Each part records which cache holds its pictures. `setup` is the setup the
    /// array was built on, which tells a remodel the look draws with its own parts.
    pub(super) fn build_object_meshes(
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
        let chosen =
            dereth_client_runtime::models::parts_for_look(world, &files, &identity, setup, array);
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
                None => {
                    match dereth_client_runtime::models::colours_for_look(&files, &identity, part) {
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
                    }
                }
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
    pub(super) fn build_part_meshes(
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
    pub(super) fn build_part_meshes_coloured(
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
    pub(super) fn render_frame_of(&self, ws: &WorldState, pos: Position) -> Frame {
        world_step::render_frame_of(ws, self.cfg.landblock, pos)
    }

    /// [`object_shared_distance`] for the local body, measured from its render-space origin.
    pub(super) fn character_shared_distance(
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
}

/// Rebuild every chunked batch's [`StaticBatch::active`] from the levels
/// [`select_levels`] just chose.
///
/// The order of the source buffer is preserved, so a placement's level-*n* mesh is drawn where
/// its level-0 mesh was: with every placement at level 0 and no `gfxobj_id == 0` level in the
/// way, this reproduces [`StaticBatch::vertices`] exactly.
pub(super) fn assemble_batches(batches: &mut [StaticBatch], placements: &[DegradePlacement]) {
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
pub(super) fn append_billboarded(out: &mut Vec<u8>, src: &[u8], draw: &Frame) {
    let stride = OBJECT_VERTEX_STRIDE;
    let rot = dereth_terrain::math::l2g(draw.rotation);
    for v in src.chunks_exact(stride) {
        let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
        let local = Vec3::new(f(0), f(4), f(8));
        let w = dereth_terrain::math::localtoglobal(draw, local);
        out.extend_from_slice(&w.x.to_le_bytes());
        out.extend_from_slice(&w.y.to_le_bytes());
        out.extend_from_slice(&w.z.to_le_bytes());
        // The normal turns with the billboard, as the D3D world matrix would turn it.
        let n = dereth_terrain::math::localtoglobalvec(rot, Vec3::new(f(12), f(16), f(20)));
        out.extend_from_slice(&n.x.to_le_bytes());
        out.extend_from_slice(&n.y.to_le_bytes());
        out.extend_from_slice(&n.z.to_le_bytes());
        out.extend_from_slice(&v[OBJECT_DIFFUSE_OFFSET..stride]);
    }
}

/// The bounding sphere of a baked buffer's positions: the centre of the axis
/// box and the farthest vertex from it. Only the object lighting's reach test reads it.
pub(super) fn vertex_sphere(vertices: &[u8]) -> (Vec3, f32) {
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
pub(super) fn drawn_vertices(b: &StaticBatch) -> &[u8] {
    if b.chunks.is_empty() {
        &b.vertices
    } else {
        &b.active
    }
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
pub(super) fn draw_part(
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
                dereth_terrain::consts::S_ALPHA_DELAY_MASK,
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

/// Shared submission for outdoor opaque/blended, indoor ordinary statics, and alpha flush.
/// The geometry accessor remains independent: picking/statistics and LOD retain every face.
/// Building mesh drawing skips nontextured subsets before either drawing or alpha enqueue.
pub(super) fn submit_static_batch(
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
pub(super) fn submit_static_batch_with(
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

pub(super) fn static_subset_visible(batch: &StaticBatch) -> bool {
    // Same authenticated default =1 as draw_env_cell; no invented UI preference.
    // Cell statics are ordinary objects, not environment-cell geometry, hence is_env_cell=false.
    dereth_world_render::objects::draw::should_draw_mesh_subset(
        batch.surface_type,
        true,
        false,
        batch.building_pass,
    )
}

pub(super) fn static_alpha_list_member(batch: &StaticBatch) -> bool {
    static_subset_visible(batch) && is_alpha_list_member(&batch.key)
}

/// Whether a static batch goes on the **alpha** list: it blends without an alpha test, and
/// is not a clip-mapped one "Multiple Pass Alpha" puts on the clip list instead.
pub(super) fn static_alpha_entry(
    batch: &StaticBatch,
    multi_pass_alpha: bool,
    detail: bool,
) -> bool {
    static_alpha_list_member(batch) && !(multi_pass_alpha && static_multipass_member(batch, detail))
}

/// The other of the two lists a non-opaque subset can be appended to: the alpha-**tested**
/// cut-outs (catalogue rows 7-9), which keep their depth write.
///
/// `alpha_blend && alpha_test` and [`is_alpha_list_member`]'s `alpha_blend && !alpha_test`
/// partition the `BlockDraw::blended` bucket, which is keyed on `alpha_blend` alone.
pub(super) fn static_clip_list_member(batch: &StaticBatch) -> bool {
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
pub(super) fn static_multipass_member(batch: &StaticBatch, detail_installed: bool) -> bool {
    use dereth_terrain::consts::S_ALPHA_DELAY_MASK;
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
pub(super) fn submit_part_mesh(
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
        let rot = place.map(|f| dereth_terrain::math::l2g(f.rotation));
        for (i, (pt, u, v)) in g.vertices.iter().enumerate() {
            let w = match place {
                Some(f) => dereth_terrain::math::localtoglobal(f, *pt),
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
                Some(m) => dereth_terrain::math::localtoglobalvec(m, n),
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
