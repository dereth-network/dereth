//! Object passes and ordered part selection.

use super::*;

impl SceneDraw {
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
    pub(super) fn draw_object_pass(
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
        let interior_seen = |cell: Option<CellId>, handle: Option<dereth_physics::PhysHandle>| {
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
        // Whether a part is drawn by this pass, and if so with which lighting (`true` is a land
        // cell's) and by which cell's object list, which picks the cone it is tested against.
        let wanted_light = |cell: Option<CellId>, handle: Option<dereth_physics::PhysHandle>| {
            let outdoors = cell.is_some_and(dereth_physics::landdefs::is_outdoors);
            // The converse crossing matters too. An indoor-origin door can own
            // an outdoor shadow (the villa courtyard door does): its part crosses the room's
            // outdoor opening, so it is registered in the land cells outside as well. Landscape
            // drawing reaches that landcell's shadow list before portal traversal clears depth,
            // regardless of its origin, and whatever detail level the building's shell is
            // drawing. That land cell has no portal view, so the part is coned against the
            // screen.
            let outdoor_shadow = || {
                handle
                    .and_then(|h| ws.character.as_ref().and_then(|c| c.world.get(h)))
                    .and_then(|body| {
                        body.shadow_objects
                            .iter()
                            .find(|shadow| {
                                shadow.cell_present
                                    && dereth_physics::landdefs::is_outdoors(shadow.cell_id)
                            })
                            .map(|shadow| shadow.cell_id)
                    })
            };
            match phase {
                ObjectPhase::All => {
                    if outdoors {
                        (!viewer_inside).then_some((true, cell))
                    } else if let Some(land) = outdoor_shadow().filter(|_| !viewer_inside) {
                        // An outdoor viewer's landscape walk draws every land cell's objects,
                        // so a part registered outside is drawn there even when no opening of
                        // its building is drawn this frame.
                        Some((true, Some(land)))
                    } else {
                        interior_seen(cell, handle).then_some((false, cell))
                    }
                }
                ObjectPhase::Outdoors => {
                    if outdoors {
                        Some((true, cell))
                    } else if let Some(land) = outdoor_shadow() {
                        Some((true, Some(land)))
                    } else {
                        interior_seen(cell, handle).then_some((false, cell))
                    }
                }
                ObjectPhase::Interior => interior_seen(cell, handle).then_some((false, cell)),
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
            let Some((outdoors, list_cell)) = wanted_light(draw_cell, o.sim.physics_handle) else {
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
                    placed: false,
                    index: i,
                    part,
                    outdoors,
                    before_depth_clear: phase == ObjectPhase::Outdoors,
                    cell: list_cell,
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

        // --- the placed statics that play a default animation -----------------------------
        // Static drawing draws them with their land cell's or interior cell's objects, from
        // the parts this frame's static tick posed; `refresh_part_levels` chose each part's
        // level and draw frame. They carry no object id.
        for block in self.blocks.values() {
            for h in &block.hosts {
                let Some(meshes) = h.meshes.as_ref() else {
                    continue;
                };
                let Some((outdoors, list_cell)) = wanted_light(Some(h.cell), None) else {
                    continue;
                };
                for (i, pl) in meshes.iter().enumerate() {
                    let Some(part) = h.driver.part_array.parts.get(i) else {
                        continue;
                    };
                    let level = h.part_levels.get(i).copied().unwrap_or(0);
                    let meshes = pl.at(level);
                    if part.no_draw() || meshes.is_empty() {
                        continue;
                    }
                    subs.push(PartSubmission {
                        object: None,
                        placed: true,
                        index: i,
                        part,
                        outdoors,
                        before_depth_clear: phase == ObjectPhase::Outdoors,
                        cell: list_cell,
                        drawing_sphere: pl.drawing_sphere_at(level),
                        draw_pos: h.part_draw_pos.get(i).copied().unwrap_or(part.pos),
                        cypt: h.part_cypt.get(i).copied().unwrap_or(0.0),
                        meshes,
                    });
                }
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
        let body = body_light.and_then(|(outdoors, list_cell)| {
            ws.character
                .as_ref()
                .map(|c| (c.driver(), outdoors, list_cell))
        });
        if let Some((driver, outdoors, list_cell)) = body.as_ref() {
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
                    placed: false,
                    index: i,
                    part,
                    outdoors: *outdoors,
                    before_depth_clear: phase == ObjectPhase::Outdoors,
                    cell: *list_cell,
                    // As above. The local body's level is always 0.
                    drawing_sphere: pl
                        .drawing_sphere_at(self.character_part_levels.get(i).copied().unwrap_or(0)),
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
            dereth_world_render::objects::parts::insertion_sort_by_cypt_key(&mut subs, |s| s.cypt);
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
        let cone_view =
            dereth_world_render::cells::clip::ViewPoly::full_screen(eye.width, eye.height, &eye);
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
        // The blended static batches: the landscape's still waiting for a flush, taken from
        // its queue here, and whatever cell drawing queued. Sorted far to near and merged in
        // after the parts and the particles, so each goes onto the alpha list in its place.
        let detail_on = self
            .current_detail(dereth_world_render::detail::DetailClass::Building)
            .is_some();
        let mut statics = std::mem::take(&mut *self.frame_static_blend.borrow_mut());
        for key in std::mem::take(&mut *self.frame_alpha_pending.borrow_mut()) {
            let Some(block) = self.blocks.get(&key) else {
                continue;
            };
            for (i, b) in block.blended.iter().enumerate() {
                // A batch whose selected levels draw nothing has nothing to queue.
                if !static_alpha_entry(b, multi_pass_alpha, detail_on)
                    || drawn_vertices(b).is_empty()
                {
                    continue;
                }
                let c = Vec3::new(
                    b.sphere.0.x + block.origin.0,
                    b.sphere.0.y + block.origin.1,
                    b.sphere.0.z,
                );
                statics.push(StaticBlendRef {
                    source: StaticBlendSource::Block(key),
                    batch: i,
                    cypt: c.sub(ws.camera.position).mag2().sqrt(),
                });
            }
        }
        dereth_world_render::objects::parts::insertion_sort_by_cypt_key(&mut statics, |r| r.cypt);
        // A cell's batches are found through the traversal's own placement map.
        let placed_cells = statics
            .iter()
            .any(|r| matches!(r.source, StaticBlendSource::Cell(_)))
            .then(|| self.traversal_cells(ws).1);
        let sub_cypts: Vec<f32> = subs.iter().map(|s| s.cypt).collect();
        let ready_cypts: Vec<f32> = ready.iter().map(|r| r.cypt).collect();
        let moving =
            dereth_world_render::objects::parts::merge_far_to_near(&sub_cypts, &ready_cypts);
        let moving_cypts: Vec<f32> = moving
            .iter()
            .map(|m| match m {
                dereth_world_render::objects::parts::Merged::First(i) => sub_cypts[*i],
                dereth_world_render::objects::parts::Merged::Second(p) => ready_cypts[*p],
            })
            .collect();
        let static_cypts: Vec<f32> = statics.iter().map(|r| r.cypt).collect();
        let order =
            dereth_world_render::objects::parts::merge_far_to_near(&moving_cypts, &static_cypts);
        for slot in order {
            let slot = match slot {
                dereth_world_render::objects::parts::Merged::First(k) => moving[k],
                dereth_world_render::objects::parts::Merged::Second(si) => {
                    // A blended static: straight onto the alpha list, in its place.
                    // LINT-OK: an index into this frame's own static queue. Not a float.
                    #[allow(clippy::cast_possible_truncation)]
                    let handle = STATIC_ENTRY | si as u32;
                    if pass.lists.push(
                        dereth_world_render::objects::alpha::AlphaList::Blend,
                        dereth_world_render::objects::alpha::AlphaEntry {
                            mesh: dereth_primitives::MeshHandle(handle),
                            surface_num: 0,
                            texture: None,
                            first_of_kind: false,
                            world_matrix: Frame::default(),
                            multipass: false,
                            range: 0..0,
                        },
                    ) {
                        pass.static_blend += 1;
                    }
                    continue;
                }
            };
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
                                dereth_terrain::consts::S_ALPHA_DELAY_MASK,
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
                            None if s.placed => None,
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
                if let Some(id) = s
                    .object
                    .or_else(|| body_id.filter(|_| !s.placed).map(ObjectId))
                {
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
                && s.object.map(|o| o.0).or(body_id.filter(|_| !s.placed)) == Some(watched)
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
            blend: pass.lists.blend_len() - pass.particle_blend - pass.static_blend,
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
                // A blended static batch's entry.
                if e.mesh.0 & STATIC_ENTRY != 0 {
                    let Some(r) = statics.get((e.mesh.0 & !STATIC_ENTRY) as usize) else {
                        continue;
                    };
                    let (batch, origin, outdoors) = match r.source {
                        StaticBlendSource::Block(key) => {
                            let Some(block) = self.blocks.get(&key) else {
                                continue;
                            };
                            (block.blended.get(r.batch), block.origin, true)
                        }
                        StaticBlendSource::Cell(id) => {
                            let Some((cell, origin)) =
                                placed_cells.as_ref().and_then(|p| p.get(&id))
                            else {
                                continue;
                            };
                            (cell.statics_blended.get(r.batch), *origin, false)
                        }
                    };
                    let Some(batch) = batch else {
                        continue;
                    };
                    let world = world_constants(&Frame::new(
                        Vec3::new(origin.0, origin.1, 0.0),
                        Quat::IDENTITY,
                    ));
                    // The landscape's batches take the outdoor pass's own set (the sun), as
                    // its flush does; a cell's take its own sphere's, as cell drawing does.
                    let set = self.cfg.object_lighting.then(|| {
                        if outdoors {
                            self.object_light_set(Vec3::ZERO, 0.0, true)
                        } else {
                            let c = Vec3::new(
                                batch.sphere.0.x + origin.0,
                                batch.sphere.0.y + origin.1,
                                batch.sphere.0.z,
                            );
                            self.object_light_set(c, batch.sphere.1, false)
                        }
                    });
                    submit_static_batch(
                        gpu,
                        per_frame,
                        &world,
                        batch,
                        set.as_deref(),
                        self.current_detail(dereth_world_render::detail::DetailClass::Building),
                    )?;
                    if outdoors {
                        let mut land = self.frame_landscape_alpha.get();
                        land.blend += 1;
                        self.frame_landscape_alpha.set(land);
                    }
                    self.frame_alpha_order
                        .borrow_mut()
                        .push(AlphaDraw::StaticBlend);
                    self.frame_blend_order
                        .borrow_mut()
                        .push((AlphaDraw::StaticBlend, r.cypt));
                    continue;
                }
                if e.mesh.0 & PARTICLE_ENTRY != 0 {
                    let Some(&(p, j)) = pass
                        .particle_queued
                        .get((e.mesh.0 & !PARTICLE_ENTRY) as usize)
                    else {
                        continue;
                    };
                    let r = &ready[p];
                    let Some(m) = self.particle_gfx.get(r.gfx).and_then(|g| g.meshes.get(j)) else {
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
}
