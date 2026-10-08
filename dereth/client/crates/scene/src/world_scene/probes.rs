//! Scene counters and read-only resource probes.

use super::*;

impl SceneDraw {
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
                let (cypt, heading) = placement_viewer_distance(p, outdoors, viewer, share_2dsq);
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
    pub(super) fn refresh_degrade_stats(&mut self) {
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
        self.stats.degrade_placements_billboarding = placements().filter(|p| p.billboards).count();
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
    pub(super) fn refresh_land_stats(&mut self, ws: &mut WorldState) {
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
        let mut push =
            |owner: EmitterOwner, origin: Vec3, m: &dereth_animation::ParticleManager| {
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

    /// Every placed static drawn posed from its own default animation: its setup and the frame
    /// each of its parts is drawn at this frame, in the renderer's space. For the tests.
    #[must_use]
    pub fn animated_static_parts(&self) -> Vec<(DataId, Vec<Frame>)> {
        self.blocks
            .values()
            .flat_map(|b| {
                b.hosts.iter().filter(|h| h.meshes.is_some()).map(|h| {
                    let setup = b.emitters.get(h.placement).map_or(DataId(0), |p| p.setup);
                    (setup, h.part_draw_pos.clone())
                })
            })
            .collect()
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

    /// What the per-part object draw of interior cells' baked statics did on the last
    /// [`Self::draw`]: the parts offered, the ones no view of the offering cell saw, and which
    /// kind of cell drew the rest.
    #[must_use]
    pub fn drawn_cell_statics(&self) -> CellStaticDrawStats {
        self.frame_cell_statics.get()
    }

    /// Every part run of a cell's static batches the last [`Self::draw`] submitted, in
    /// submission order: a run submitted twice under one pass of the frame is a part drawn
    /// twice.
    #[must_use]
    pub fn drawn_cell_static_runs(&self) -> Vec<CellStaticRunDraw> {
        self.frame_cell_runs.borrow().clone()
    }

    /// Every part of every resident interior cell's baked statics: where it is registered, its
    /// drawing sphere this frame, and which cell drew it on the last [`Self::draw`].
    #[must_use]
    pub fn cell_static_parts(&self) -> Vec<CellStaticPartProbe> {
        let first = self.frame_stamp_first.get();
        // Which other interior cells hold each part, from the cells' guest lists.
        // ORDER-OK: a lookup by part; each list is sorted below.
        let mut held: HashMap<(u32, u32), Vec<CellId>> = HashMap::new();
        for other in self.blocks.values().flat_map(|b| &b.env_cells) {
            for g in &other.objects.guests {
                held.entry(*g).or_default().push(other.id);
            }
        }
        let mut out = Vec::new();
        for block in self.blocks.values() {
            for cell in &block.env_cells {
                for (k, p) in cell.objects.parts.iter().enumerate() {
                    // LINT-OK: an index into one cell's parts. Not a float.
                    #[allow(clippy::cast_possible_truncation)]
                    let k = k as u32;
                    let mut registered: Vec<CellId> = cell
                        .objects
                        .outdoors
                        .iter()
                        .filter(|o| o.1 == k)
                        .map(|o| CellId(o.0))
                        .collect();
                    registered.extend(held.get(&(cell.id.0, k)).into_iter().flatten());
                    registered.sort_unstable();
                    let (stamp, by, _) = p.drawn.get();
                    out.push(CellStaticPartProbe {
                        cell: cell.id,
                        object: p.object,
                        part: k,
                        setup: p.setup,
                        registered,
                        sphere: Self::cell_part_sphere(cell, block.origin, p),
                        drawn_by: (stamp != 0 && stamp.wrapping_sub(first) < 0x8000_0000)
                            .then_some(CellId(by)),
                    });
                }
            }
        }
        out
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
}
