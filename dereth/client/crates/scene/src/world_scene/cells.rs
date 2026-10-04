//! Cell traversal, alpha lists and building interiors.

use super::*;

/// Portal pixels are local to the active viewport, just like the eye transform that
/// turns them back into world-space clipping planes. The device applies the viewport origin.
fn portal_screen(
    eye: &dereth_world_render::cells::clip::EyeTransform,
) -> (f32, f32, dereth_world_render::cells::clip::ViewPoly) {
    let (width, height) = (eye.width, eye.height);
    let full = dereth_world_render::cells::clip::ViewPoly::full_screen(width, height, eye);
    (width, height, full)
}

impl SceneDraw {
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
    /// superset of the geometry visible through the opening. The Z clear makes that
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
    pub(super) fn draw_inside(
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
        let vp = self.view_params(ws, vw, vh);
        let view_proj = dereth_render::camera::projection(&vp) * vp.view;
        let eye = self.eye_transform(ws, &vp);
        let (fw, fh, full) = portal_screen(&eye);

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
        // frame runs `draw_landscape` — and therefore `draw_building_interiors`' own publication —
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
        let env_detail = self.current_detail(dereth_world_render::detail::DetailClass::Environment);
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
                        self.draw_landscape(ws, gpu, per_frame, sky_per_frame, false)?;
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
                // The queue is real: `draw_landscape` fills it and does not drain it, so this
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
                                if p.other_cell_id != 0xFFFF_FFFF || is_dummy_portal(&p.vertices) {
                                    continue;
                                }
                                let screen: Vec<ScreenPoint> = p
                                    .vertices
                                    .iter()
                                    .map(|v| {
                                        let bl =
                                            dereth_physics::math::localtoglobal(&cell.frame, *v);
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
                        self.draw_cell_statics(gpu, per_frame, &cell.statics, *origin, None)?;
                    }
                }
                // Flush the alpha list at depth 0.0: the queue everything that blends was
                // put on. The interior statics' blended batches go to the interior object
                // pass's alpha list, which flushes them among the creatures' and particles'
                // by distance; their alpha-tested ones are drawn here.
                IndoorStep::FlushAlphaList => {
                    for id in view.draw_order() {
                        let Some((cell, origin)) = placed.get(&id.0) else {
                            continue;
                        };
                        self.draw_cell_statics(
                            gpu,
                            per_frame,
                            &cell.statics_blended,
                            *origin,
                            Some((id.0, ws.camera.position)),
                        )?;
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
    pub(super) fn draw_cell_statics(
        &self,
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
        batches: &[StaticBatch],
        origin: (f32, f32),
        // `Some((cell, viewer))` queues the batches that belong on the alpha list for the
        // object pass, at their distance from `viewer`, instead of drawing them here.
        defer: Option<(u32, Vec3)>,
    ) -> Result<(), RenderError> {
        if batches.is_empty() || !self.cfg.cell_statics {
            return Ok(());
        }
        let world = world_constants(&Frame::new(
            Vec3::new(origin.0, origin.1, 0.0),
            Quat::IDENTITY,
        ));
        let detail_on = self
            .current_detail(dereth_world_render::detail::DetailClass::Building)
            .is_some();
        for (i, b) in batches.iter().enumerate() {
            if let Some((cell, viewer)) = defer {
                if static_alpha_entry(b, self.cfg.render.multi_pass_alpha, detail_on) {
                    if drawn_vertices(b).is_empty() {
                        continue;
                    }
                    let c = Vec3::new(
                        b.sphere.0.x + origin.0,
                        b.sphere.0.y + origin.1,
                        b.sphere.0.z,
                    );
                    self.frame_static_blend.borrow_mut().push(StaticBlendRef {
                        source: StaticBlendSource::Cell(cell),
                        batch: i,
                        cypt: c.sub(viewer).mag2().sqrt(),
                    });
                    continue;
                }
            }
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
            let detail = self.current_detail(dereth_world_render::detail::DetailClass::Building);
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
                submit_static_batch_with(gpu, per_frame, &world, b, set.as_deref(), detail, true)?;
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
    pub(super) fn traversal_cells(&self, ws: &WorldState) -> (TraversalCells, PlacedCells<'_>) {
        use dereth_world_render::cells::portal_view::{CellPortal as ViewPortal, TraversalCell};
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
                                p.plane_normal
                                    .y
                                    .mul_add(local.y, p.plane_normal.z.mul_add(local.z, p.plane_d)),
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
    pub(super) fn draw_env_cell(
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
    pub(super) fn flush_alpha_list(
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
            static_alpha_entry(b, self.cfg.render.multi_pass_alpha, detail.is_some())
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
    pub(super) fn drain_multipass_pending(
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
    pub(super) fn flush_pending_alpha_list(
        &self,
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
    ) -> Result<(), RenderError> {
        // The clip list drains first. The object pass has already drawn these second passes
        // between its own clip and blend entries, and has taken the landscape's blended
        // batches into its alpha list; this finds both queues empty unless no object pass ran.
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
    pub(super) fn draw_building_interiors(
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
        use dereth_world_render::cells::clip::{copy_view, get_clip, ScreenPoint};
        use dereth_world_render::cells::portal_view::{
            build_draw_portals_only, construct_view_clipped, construct_view_from, draw_portal,
            sidedness, Sidedness,
        };
        use dereth_world_render::objects::buildings::portal_pass;

        let (vw, vh) = gpu.size();
        let view = self.view_params(ws, vw, vh);
        let view_proj = dereth_render::camera::projection(&view) * view.view;
        // The one
        // full-screen view polygon the outdoor pass clips its openings against, with its
        // four eye planes; nothing on **this** path reads them (the objects
        // this pass draws are a building's own cells), and the object cull that does is the
        // phase-3 loop in `draw`.
        let eye = self.eye_transform(ws, &view);
        let (fw, fh, full) = portal_screen(&eye);
        // A building sets sunlight use to 0 before drawing its cells, so they
        // take `minimize_envcell_lighting`'s dynamic set.
        let cell_set = self.cfg.object_lighting.then(|| self.envcell_light_set());
        // Environment-cell drawing installs the environment detail surface before every
        // cell mesh it draws.
        let env_detail = self.current_detail(dereth_world_render::detail::DetailClass::Environment);

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
                let step = draw_portal(poly, &b.portals, d, mode, &|_| opened.is_some(), &|id| {
                    cells.contains_key(&id)
                });

                // Unless the mode is 2, the portal polygon is drawn, with its flag set when the mode is 1 —
                // the depth stamp.
                if step.stamp.is_some()
                    && self.cfg.portal_depth_stamp
                    && !is_dummy_portal(&p.vertices)
                {
                    let quad: Vec<[f32; 4]> = clipped.iter().map(|s| s.to_clip(fw, fh)).collect();
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
                    self.draw_cell_statics(gpu, per_frame, &cell.statics, *origin, None)?;
                }
                for id in &order {
                    let Some((cell, origin)) = placed.get(id) else {
                        continue;
                    };
                    self.draw_cell_statics(
                        gpu,
                        per_frame,
                        &cell.statics_blended,
                        *origin,
                        Some((*id, ws.camera.position)),
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod viewport_tests {
    use super::portal_screen;
    use dereth_primitives::{Frame, Vec3, Viewport};
    use dereth_render::camera::{projection, view_from_frame, ViewParams};
    use dereth_world_render::cells::clip::{
        copy_view, poly_clip_finish, EyeTransform, ScreenPoint,
    };
    use dereth_world_render::cells::cull::{viewcone_check, viewer_near_plane, Bounding};

    /// Behaviour: rendering.interior.each-reached-cell-draws-through-its-own-portal-view
    #[test]
    fn portal_planes_follow_the_active_viewport_instead_of_the_back_buffer() {
        for viewport in [
            Viewport {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            Viewport {
                x: 0,
                y: 28,
                width: 492,
                height: 472,
            },
            Viewport {
                x: 123,
                y: 87,
                width: 360,
                height: 280,
            },
        ] {
            #[allow(clippy::cast_precision_loss)] // bounded test viewport sizes
            let (width, height) = (viewport.width as f32, viewport.height as f32);
            let view = ViewParams {
                view: view_from_frame(&Frame::default()),
                fov_y_rad: 1.0,
                aspect: width / height,
                viewport,
                ..Default::default()
            };
            let matrix = projection(&view);
            let diagonal = matrix.to_cols_array();
            let eye = EyeTransform {
                viewpoint: Vec3::ZERO,
                inv_view: view.view.inverse().to_cols_array(),
                proj_11: diagonal[0],
                proj_22: diagonal[5],
                width,
                height,
            };
            // A narrow opening straight ahead. Both drawing paths use this setup;
            // no simulated object placement or graphics device is needed for its planes.
            let (fw, fh, full) = portal_screen(&eye);
            let points: Vec<_> = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
                .into_iter()
                .map(|(x, z)| {
                    let q = matrix * view.view * glam::Vec4::new(x, z, 5.0, 1.0);
                    let clip = q.to_array();
                    let pixel = ScreenPoint::from_clip(clip, fw, fh);
                    let restored = pixel.to_clip(fw, fh);
                    for (actual, expected) in restored.into_iter().zip(clip) {
                        assert!((actual - expected).abs() < 1e-5);
                    }
                    pixel
                })
                .collect();
            let clipped = poly_clip_finish(&points, &full);
            let opening = copy_view(&clipped, &eye).expect("visible opening");
            let near = viewer_near_plane(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 0.1);
            assert_eq!(
                viewcone_check(Vec3::new(0.0, 5.0, 0.0), 0.1, &near, &opening.planes),
                Bounding::EntirelyInside,
                "the object behind the opening must remain visible in {viewport:?}"
            );
            assert_eq!(
                viewcone_check(Vec3::new(2.0, 5.0, 0.0), 0.1, &near, &opening.planes),
                Bounding::Outside,
                "objects outside the opening must still be culled"
            );
        }
    }
}
