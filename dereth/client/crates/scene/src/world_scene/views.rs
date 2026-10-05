//! View transforms, draw dispatch and portal probes.

use super::*;

impl SceneDraw {
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
    pub(super) fn eye_transform(
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
    pub(super) fn viewer_near_plane(&self, ws: &WorldState) -> dereth_world_render::Plane {
        let f = ws.camera.frame();
        dereth_world_render::cells::cull::viewer_near_plane(
            f.origin,
            dereth_terrain::math::get_vector_heading(&f),
            dereth_render::camera::ZNEAR,
        )
    }

    /// The inputs to [`Self::object_light_set`] for one part submission: the drawing sphere
    /// view-cone checking leaves as the local object centre and radius
    /// (the level's own sphere, scaled and placed by `draw_pos` as [`Self::part_cone`] places
    /// it), and the pass the object's cell puts it in.
    pub(super) fn submission_light_set(&self, s: &PartSubmission<'_>) -> Vec<D3dLight> {
        use dereth_world_render::cells::cull::object_scale;
        let (centre, radius) = match s.drawing_sphere {
            Some((c, r)) => {
                let scale = object_scale(s.part.gfxobj_scale);
                let scaled = Vec3::new(c.x * scale, c.y * scale, c.z * scale);
                (
                    dereth_terrain::math::localtoglobal(&s.draw_pos, scaled),
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
    pub(super) fn part_cone(
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
        let world = dereth_terrain::math::localtoglobal(&s.draw_pos, scaled);
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
    pub(super) fn part_cone_views(
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
        let world = dereth_terrain::math::localtoglobal(&s.draw_pos, scaled);
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

    /// The viewer position used for lighting. The client copies the player's
    /// position into it every frame, and light insertion
    /// measures every light's sort key from its origin. The cell id and the frame in render
    /// space; the free camera when there is no body.
    pub(super) fn player_origin(&self, ws: &WorldState) -> (u32, Frame) {
        match ws.character.as_ref() {
            Some(c) => {
                let p = c.position();
                (p.cell.0, self.render_frame_of(ws, p))
            }
            None => (0, Frame::new(ws.camera.position, Quat::IDENTITY)),
        }
    }

    /// Draw one frame of the world in normal render mode.
    ///
    /// The outdoor pass itself is `Self::draw_landscape`, because retail's normal-mode render
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
    pub(super) fn draw_in_viewport(
        &self,
        ws: &WorldState,
        gpu: &mut Gpu,
    ) -> Result<(), RenderError> {
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
        self.frame_static_blend.borrow_mut().clear();
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
            let _ = self.draw_landscape(ws, gpu, &per_frame, &sky_per_frame, outside)?;
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
                        let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
                        clip[k] = view_proj * world * glam::Vec4::new(f(0), f(4), f(8), 1.0);
                    }
                    if clip.iter().any(|c| c.w <= 1.0e-4) {
                        continue;
                    }
                    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
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

/// The environment-cell membership test, as this build can express
/// it: a batch is on the alpha list when it blends and is **not** alpha-tested.
///
/// A clip-mapped surface (material rows 7 and 8) has `alpha_blend` *and* `alpha_test` set and keeps its
/// depth write; it is drawn in place. A translucent one (rows 3-6 and 10-13) has `alpha_blend`
/// without `alpha_test` and has the depth write off; it is queued. So alpha blending
/// without alpha testing defines the queue, exactly the set with `z_write == false`.
#[must_use]
pub(super) fn is_alpha_list_member(key: &dereth_render::PipelineKey) -> bool {
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
pub(super) fn is_dummy_portal(vertices: &[Vec3]) -> bool {
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
        normal_scale: normal_scale(scale),
        ..PerDrawConstants::identity()
    }
}

/// A part's model-space normal as a baked static carries it when its mesh is drawn at
/// `scale`: through the inverse transpose of the scale, `n / scale`, renormalised. The
/// placement's rotation is applied after. A uniform scale leaves the normal as it is, exactly
/// as the normalising lighting would see it.
#[must_use]
pub fn baked_normal(n: Vec3, scale: Vec3) -> Vec3 {
    if scale.x == scale.y && scale.y == scale.z {
        return n;
    }
    let m = Vec3::new(n.x / scale.x, n.y / scale.y, n.z / scale.z);
    let len = m.magnitude();
    if len > 0.0 && len.is_finite() {
        Vec3::new(m.x / len, m.y / len, m.z / len)
    } else {
        n
    }
}

/// [`PerDrawConstants::normal_scale`] for a part drawn at `scale`: lighting takes the normal
/// through the inverse transpose of the world matrix, which for a rotation times this scale is
/// the world matrix applied to `n / scale²`. A uniform scale changes only the normal's length,
/// which the normalisation removes, so it keeps the plain transform and draws as it always did.
#[must_use]
pub fn normal_scale(scale: Vec3) -> [f32; 4] {
    if scale.x == scale.y && scale.y == scale.z {
        return [0.0; 4];
    }
    let inv = |v: f32| {
        let q = 1.0 / (v * v);
        if q.is_finite() {
            q
        } else {
            0.0
        }
    };
    [inv(scale.x), inv(scale.y), inv(scale.z), 1.0]
}

/// The axis conversion swaps forward and up: client `(x, y, z)`
/// reaches D3D as `(x, z, y)`. `dereth_render::camera::view_from_frame` documents that its input
/// is already in D3D order, so the world matrix is where the swap belongs.
pub(super) fn swap_zup_to_d3d() -> glam::Mat4 {
    glam::Mat4::from_cols(
        glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 1.0, 0.0),
        glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
        glam::Vec4::W,
    )
}

/// A [`Frame`] as a glam column-vector matrix: `M v = R v + t`.
pub(super) fn frame_matrix(f: &Frame) -> glam::Mat4 {
    let m = dereth_terrain::math::l2g(f.rotation).0;
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
