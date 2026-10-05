//! Distance-based detail selection and emitter degradation.

use super::*;

impl SceneDraw {
    /// Update the frame-rate window, then update the adaptive degrade multiplier.
    ///
    /// **The two halves are one call here where the client has them a frame apart**, and that
    /// is not a shortcut: the client measures at frame end, at the *bottom* of
    /// frame *n*, and reads the result during world update, at the *top* of frame *n+1*, so
    /// what [`Self::calc_deg_level`] sees is a window ending with the duration of the previous frame.
    /// `dt` here is `time(n) - time(n-1)`, which is exactly that
    /// duration, so pushing it at the top of frame *n* leaves the ring holding the same twenty
    /// numbers the client's last-frame-times ring holds at the same moment.
    ///
    /// **The headless gate.** The whole of the loop's input is this `dt`, and this
    /// `dt` is a difference of frame timestamps, which `dereth_client_runtime::platform::clock::Timer::fixed_step` makes
    /// a fixed quantum under `--headless`. So a headless frame measures exactly
    /// `20 / (k * HEADLESS_STEP)` for the *k*th frame on every machine, the loop's trajectory
    /// is a function of the frame **count** and not of elapsed time, and the capture stays
    /// byte-identical across runs whether the governor is pinned or live. Nothing in this path
    /// reads a wall clock: the wall clock is the network's clock and is not consulted
    /// here. (`SkyScene::use_time`'s UV scroll takes the same `dt` for the same reason, so the
    /// two subsystems that are functions of elapsed time are functions of the *same* one.)
    pub(super) fn calc_deg_level(&mut self, dt: f32) {
        self.degrade.frame_rate.push(f64::from(dt));
        let fps = self.degrade.frame_rate.fps();
        self.degrade.governor.use_time(fps);
        self.degrade.frames += 1;
    }

    /// For every baked static
    /// placement in the window, then re-assemble the batches whose choice changed.
    ///
    /// The client runs this per part per frame and issues one draw per part; this build
    /// batches by surface, so the same decision is made per part and the *assembly* is what
    /// costs. It is deliberately not a per-frame rebake: the level a placement draws changes
    /// only when it crosses a threshold, so the assembled buffers are rebuilt on the frames
    /// that cross one and reused on every frame in between. A walk through Holtburg changes a
    /// double-digit number of levels per frame out of thousands of placements.
    ///
    /// It must run **after** [`Self::calc_deg_level`], because the bias that frame's
    /// thresholds slide by is the one the loop just produced, and after the re-centre, because
    /// `block.origin` moves with it.
    pub(super) fn refresh_degrade_levels(&mut self, ws: &mut WorldState) -> u32 {
        let globals = self.degrade_globals(ws);
        let cam = detail_viewer(ws);
        // Every baked placement belongs to an ordinary object, never a particle emitter.
        let share_2dsq = self.degrade.governor.level().share_distance_2dsq(false);
        let mut switches = 0u32;
        let mut turns = 0u32;
        let mut assemblies = 0u32;
        for block in self.blocks.values_mut() {
            let viewer = Vec3::new(cam.x - block.origin.0, cam.y - block.origin.1, cam.z);
            let first = !block.degrade_assembled;
            let (changed, n, t) =
                select_levels(&mut block.degrade, true, viewer, share_2dsq, &globals);
            switches += n;
            turns += t;
            if changed || first {
                assemble_batches(&mut block.opaque, &block.degrade);
                assemble_batches(&mut block.blended, &block.degrade);
                assemblies += 1;
            }
            for cell in &mut block.env_cells {
                let (changed, n, t) =
                    select_levels(&mut cell.degrade, false, viewer, share_2dsq, &globals);
                switches += n;
                turns += t;
                if changed || first {
                    assemble_batches(&mut cell.statics, &cell.degrade);
                    assemble_batches(&mut cell.statics_blended, &cell.degrade);
                    assemblies += 1;
                }
            }
            block.degrade_assembled = true;
        }
        // A billboard turn is a reassembly the way a level change is, so the two
        // costs are reported side by side rather than folded together: `degrade_switches`
        // counts levels that changed and these two count the turns and reassemblies.
        //
        // **They accumulate, and [`Self::update`] zeroes them at the top of the frame**, because
        // this function runs **twice** on a frame that also streams -- once from `update` and
        // once from `stream`, for the reason recorded at the second call site. Assigning
        // would let `stream`'s second pass (which finds nothing changed, correctly) overwrite
        // the real count with 0 on exactly the frames a block arrives.
        self.stats.degrade_billboard_turns += turns as usize;
        self.stats.degrade_assemblies += assemblies as usize;
        switches
    }

    /// Pick this frame's degrade level for every part of every
    /// thing that moves: the server's objects and the local body.
    ///
    /// For each part, the viewer-distance update computes the distance and the viewer heading.
    /// A degrading part of an object other than the player then takes its level and mode from
    /// the degrade lookup at that distance divided by the graphics-object scale's z; any other
    /// part uses level 0. The draw frame is calculated from the mode and the heading, and the
    /// draw stops if the selected level's graphics object is missing.
    ///
    /// This is `dereth_world_render::objects::parts::select_level`'s production caller, with
    /// the scale divide and the player exemption in it.
    ///
    /// It must run **after** the object step ([`dereth_client_runtime::world_step::step_objects`]), which is what writes `part.pos` from
    /// this frame's animation frame, and after [`Self::calc_deg_level`], whose bias this
    /// frame's thresholds slide by — the same two orderings
    /// [`Self::refresh_degrade_levels`] has, for the same two reasons.
    ///
    /// **The billboarding mode is kept too.**
    /// `select_level` fills `PartDraw::draw_pos` with the draw frame calculated from the part
    /// position, the degrade mode and the viewer heading.
    /// It is stored per part here and submitted by `draw_part`, gated on
    /// [`SceneConfig::part_billboards`] so that submitting `part.pos` instead is a control arm.
    pub(super) fn refresh_part_levels(&mut self, ws: &mut WorldState) -> u32 {
        let globals = self.degrade_globals(ws);
        let billboards = self.cfg.part_degrade_levels && self.cfg.part_billboards;
        let cam = detail_viewer(ws);
        let share = self.degrade.governor.level();
        let mut switches = 0u32;
        let mut resident = 0usize;
        let mut drawn_tris = 0usize;
        let mut drawn_batches = 0usize;
        let (mut with_record, mut degraded, mut culled) = (0usize, 0usize, 0usize);
        // The same "asked / answered / acted on" three-state shape the degrade
        // counters have: how many parts' selected level asks for a billboarding mode, and how
        // many of those actually got a different frame.
        let (mut part_billboards, mut part_turns) = (0usize, 0usize);

        for (id, o) in &ws.objects {
            let shared = object_shared_distance(ws, o, cam, &share);
            let d = self
                .object_draws
                .get_mut(id)
                .expect("every world object has its drawing half");
            d.part_levels.resize(d.meshes.len(), 0);
            d.part_draw_pos.resize(d.meshes.len(), Frame::default());
            d.part_cypt.resize(d.meshes.len(), 0.0);
            for (i, pl) in d.meshes.iter().enumerate() {
                resident += pl.triangles_held();
                if pl.info.is_some() {
                    with_record += 1;
                }
                let driver = o.sim.driver.borrow();
                let Some(part) = driver.part_array.parts.get(i) else {
                    continue;
                };
                let chosen = part_level(pl, part, o.is_player, cam, shared, &globals);
                let (level, mode, draw_pos) = (chosen.level, chosen.mode, chosen.draw_pos);
                // The fourth thing kept out of the one viewer-distance update
                // this function already runs. Unconditional: the sort key is measured whether
                // or not `part_depth_sort` is set, so that the control arm and the live arm
                // differ only in whether `draw` *uses* it.
                d.part_cypt[i] = chosen.cypt;
                // With the switch clear this is `part.pos`, the unbillboarded frame.
                d.part_draw_pos[i] = if billboards { draw_pos } else { part.pos };
                if mode != dereth_world_render::objects::degrade::DegradeMode::None {
                    part_billboards += 1;
                    if d.part_draw_pos[i] != part.pos {
                        part_turns += 1;
                    }
                }
                if d.part_levels[i] != level {
                    d.part_levels[i] = level;
                    switches += 1;
                }
                if level != 0 {
                    degraded += 1;
                    if pl.at(level).is_empty() {
                        culled += 1;
                    }
                }
                if !o.drawn || part.no_draw() {
                    continue;
                }
                drawn_tris += pl.triangles_at(level);
                drawn_batches += pl.at(level).len();
            }
        }
        self.stats.server_object_triangles = drawn_tris;
        self.stats.server_object_batches = drawn_batches;
        self.stats.server_object_triangles_resident = resident;

        let (mut body_tris, mut body_batches, mut body_parts, mut body_resident) =
            (0usize, 0usize, 0usize, 0usize);
        if let Some(c) = ws.character.as_ref() {
            let shared = self.character_shared_distance(ws, cam, &share);
            let driver = c.driver();
            self.character_part_levels
                .resize(self.character_parts.len(), 0);
            self.character_part_draw_pos
                .resize(self.character_parts.len(), Frame::default());
            self.character_part_cypt
                .resize(self.character_parts.len(), 0.0);
            for (i, pl) in self.character_parts.iter().enumerate() {
                body_resident += pl.triangles_held();
                if pl.info.is_some() {
                    with_record += 1;
                }
                let Some(part) = driver.part_array.parts.get(i) else {
                    continue;
                };
                // `true`: this is the local player, the one object the viewer-distance update
                // exempts. Every level is on the device; nothing but this flag keeps him at 0.
                let chosen = part_level(pl, part, true, cam, shared, &globals);
                let (level, mode, draw_pos) = (chosen.level, chosen.mode, chosen.draw_pos);
                // The player is exempt from *degrading*, not from being
                // sorted: the viewer-distance update measures the distance before it tests
                // whether the object is the player.
                self.character_part_cypt[i] = chosen.cypt;
                // The player is exempt from *degrading*, not from *billboarding*:
                // `select_level` short-circuits to level 0 and reads level 0's own mode, so a
                // body part whose level-0 record says mode 5 still turns. That is the client's
                // own arithmetic and it is why this is not hard-coded to `part.pos`.
                self.character_part_draw_pos[i] = if billboards { draw_pos } else { part.pos };
                if mode != dereth_world_render::objects::degrade::DegradeMode::None {
                    part_billboards += 1;
                    if self.character_part_draw_pos[i] != part.pos {
                        part_turns += 1;
                    }
                }
                if self.character_part_levels[i] != level {
                    self.character_part_levels[i] = level;
                    switches += 1;
                }
                if level != 0 {
                    degraded += 1;
                }
                if part.no_draw() {
                    continue;
                }
                let t = pl.triangles_at(level);
                body_tris += t;
                body_batches += pl.at(level).len();
                if t > 0 {
                    body_parts += 1;
                }
            }
        }
        self.stats.character_triangles = body_tris;
        self.stats.character_batches = body_batches;
        self.stats.character_parts = body_parts;
        self.stats.character_triangles_resident = body_resident;

        self.stats.part_degrade_parts = with_record;
        self.stats.part_degrade_parts_degraded = degraded;
        self.stats.part_degrade_parts_culled = culled;
        // **After both loops**: these two are summed over the server's objects and
        // the local body, and publishing them between the two would silently drop the body's half.
        self.stats.part_billboards = part_billboards;
        self.stats.part_billboards_turned = part_turns;
        switches
    }

    /// The current degrade inputs read this frame.
    ///
    /// The bias is `deg_mul` when automatic degrades are on and the user-supplied degrade bias otherwise, so a **pinned**
    /// governor is expressed the way the client expresses one: automatic degrades off
    /// and the pinned value standing in as the user-supplied bias. With
    /// [`dereth_terrain::consts::PINNED_DEG_MUL`] at 0 the two spellings agree, and the
    /// point of writing it out is that they keep agreeing if it ever moves.
    #[must_use]
    pub fn degrade_globals(
        &self,
        ws: &WorldState,
    ) -> dereth_world_render::objects::degrade::DegradeGlobals {
        let g = &self.degrade.governor;
        dereth_world_render::objects::degrade::DegradeGlobals {
            degrades_disabled: ws.degrades_disabled(),
            // -1 in every running frame, because retail has
            // no writer for it and neither does this build -- see [`Self::set_force_level`].
            force_level: self.force_level,
            auto_update_deg_mul: g.auto,
            deg_mul: g.deg_mul,
            // The pinned degrade multiplier comes from
            // `Render.GraphicsPerformance`, which `get_degrade` reads when
            // `auto_update_deg_mul` is clear. With the governor pinned the two agree at
            // `PINNED_DEG_MUL`; a profile that names the preference moves it, which is the
            // whole of what the slider is for. A pinned governor at a *non*-default
            // `PINNED_DEG_MUL` still wins, because that is this build's determinism
            // decision and not a user setting.
            user_bias: if g.auto {
                0.0
            } else if g.deg_mul == dereth_terrain::consts::PINNED_DEG_MUL {
                self.cfg.render.graphics_performance
            } else {
                g.deg_mul
            },
            // The degrade-distance preference -- `Render.DegradeDistance`, whose
            // initial value is the 50.0 `DegradeGlobals::default()` carried -- on every
            // world, the 2005 one included (CD-026).
            degrade_distance: self.cfg.render.degrade_distance,
        }
    }
}

/// The one `degrade_distance` this build hands
/// for a whole `ParticleManager`: `f32::max` over its emitters, floored at
/// the native no-record default of 100.0.
///
/// **The shared particle-distance policy differs.** The client asks per *emitter*: particle
/// preparation's first act is the should-draw-particles test on the emitter's physics object
/// and `degrade_distance`, where
/// `degrade_distance` is what emitter setup stored for that emitter
/// alone. So an emitter whose own `GfxObjDegradeInfo` cuts off at 40 m keeps emitting,
/// simulating and drawing out to whatever the widest emitter sharing its manager asks for.
///
/// **The narrower ones are not cut at draw by their LOD terminator at the same distance.**
/// The two cut-offs are computed from different quantities:
///
/// | | compared against | measured from |
/// |---|---|---|
/// | the should-draw-particles test | the second-last degrade level's maximum distance, **raw** | the object's origin |
/// | the draw ([`crate::particles::prepare`]) | `get_degrade`'s bands, over `(d - 50.0).max(0)` | each **particle**, over its own scale |
///
/// The renderer's degrade-distance bias is **50.0**, so the draw's terminator sits about 50 m *beyond*
/// the emitter's own cut-off rather than at it. The emitter test is the tighter of the two in
/// the client and it is the one this build is not making.
///
/// The fix is a `dereth-animation` change — `EmitterContext::should_draw` is one flag for the whole
/// manager and the emitter's own degrade-distance field (which exists but is neither written nor
/// read) is where the per-emitter number belongs — so it is not worked
/// around here. A test pins the deviation and is **expected to go red** when the seam lands.
pub(super) fn manager_degrade_distance(
    gfx: &ParticleGeometry,
    m: &dereth_animation::ParticleManager,
) -> f32 {
    m.iter()
        .map(|e| gfx.max_degrade_distance(e.info.hw_gfxobj_id))
        .fold(crate::particles::DEFAULT_DEGRADE_DISTANCE, f32::max)
}

impl EmitterDegrade {
    /// The should-draw-particles test at `own_distance` — what the client answers for this emitter.
    #[must_use]
    pub fn client_draws(&self) -> bool {
        self.cypt <= self.own_distance
    }

    /// The should-draw-particles test at `manager_distance` — what this build answers.
    #[must_use]
    pub fn this_build_draws(&self) -> bool {
        self.cypt <= self.manager_distance
    }

    /// The emitter is past its own cut-off and inside its manager's, so it is running and
    /// drawing where the client would have hidden it: the deviation, per emitter.
    #[must_use]
    pub fn drawing_past_its_own_cutoff(&self) -> bool {
        self.this_build_draws() && !self.client_draws()
    }
}

/// The point every detail level, part sort distance and particle cut-off is measured from:
/// the placed viewer, which is the eye the frame is drawn from. With no body (or before the
/// body's camera is first placed) the free camera is that eye.
///
/// **Not `ws.camera`.** The scene update runs before the camera sweep, and at that moment
/// `ws.camera` holds the unswept chase camera the update has just placed, about 4.4 m behind
/// and 2.5 m above the feet where the eye is 2.75 m behind and 2.3 m above. Measured from
/// there, everything near the player stood about 1.6 m further away than it is drawn, so at a
/// Degrade Distance of 0 the furniture beside the player dropped a level the eye's own
/// distance keeps. The placed viewer is the sweep's result from the frame before: when the
/// camera is still it is the eye exactly, and a moving camera is one frame of travel behind,
/// as the landblock window's re-centre already is.
pub(super) fn detail_viewer(ws: &WorldState) -> Vec3 {
    dereth_client_runtime::world_stream::viewpoint(&ws.character, &ws.camera)
}

/// Select the degrade level for a whole block's baked
/// placements, then the assembly of what that chose.
///
/// Returns whether any placement changed level, i.e. whether the batches have to be rebuilt.
/// `viewer` is **block-local**, like the vertices.
pub(super) fn select_levels(
    placements: &mut [DegradePlacement],
    outdoors: bool,
    viewer: Vec3,
    share_2dsq: f32,
    globals: &dereth_world_render::objects::degrade::DegradeGlobals,
) -> (bool, u32, u32) {
    let mut changed = false;
    let mut switches = 0u32;
    let mut turns = 0u32;
    for p in placements.iter_mut() {
        // The client computes the offset from the viewer to the part, then the distance
        // `|v|` and the heading `v / |v|`. Both ends are in the same block frame, so the offset is
        // a subtraction. The viewer-distance update's own tail is used rather than a
        // hand-rolled magnitude, because this path needs the
        // *heading* as well, and the degenerate `(0,0,1)` branch with it.
        let (cypt, heading) = placement_viewer_distance(p, outdoors, viewer, share_2dsq);
        // `get_degrade` at the viewer distance over the z scale.
        let z = p.scale_z;
        let dist = if z != 0.0 { cypt / z } else { cypt };
        let (level, mode) =
            dereth_world_render::objects::degrade::get_degrade(&p.info, dist, globals);
        // LINT-OK: a level index; the longest shipped record has six. Not a float conversion.
        #[allow(clippy::cast_possible_truncation)]
        let level = level as u32;
        if level != p.level {
            p.level = level;
            changed = true;
            switches += 1;
        }
        // **The mode is kept.** Draw-frame calculation turns
        // `draw_pos.frame` toward the viewer for modes 2-5, which world-space vertices in a
        // `StaticBatch` cannot do. The batches hold *part-local* vertices for exactly the
        // placements a record billboards,
        // and [`assemble_batches`] applies this frame as it copies them.
        //
        // The comparison is what keeps the cost down: a billboard whose draw frame has
        // not moved since the last assembly does not ask for one, so a camera standing still
        // reassembles nothing.
        p.mode = mode;
        if p.billboards {
            let draw =
                dereth_world_render::objects::degrade::calc_draw_frame(&p.frame, mode, heading);
            if draw != p.draw_pos {
                p.draw_pos = draw;
                changed = true;
                turns += 1;
            }
        }
    }
    (changed, switches, turns)
}

/// The cell- and object-level halves of the viewer-distance update for one world object: its
/// land cell's horizontal distance and heading when that cell is more than fifty units away
/// horizontally, otherwise the object's own distance and heading when the camera is at or past
/// its share distance horizontally, and `None` when each part measures itself. A particle
/// emitter object shares at the particle distance, anything else at the object distance.
pub(super) fn object_shared_distance(
    ws: &WorldState,
    o: &dereth_client_runtime::world_state::WorldObject,
    cam: Vec3,
    share: &dereth_world_render::degrade_loop::DegradeLevel,
) -> Option<(f32, Vec3)> {
    let outdoors = ws
        .object_draw_cell(o)
        .is_some_and(dereth_physics::landdefs::is_outdoors);
    dereth_world_render::objects::parts::object_viewer_distance(
        o.frame.origin,
        outdoors,
        cam,
        share.share_distance_2dsq(o.sim.state.is_particle_emitter()),
    )
}

/// One baked placement's viewer distance and heading: its land cell's, when `outdoors` and
/// that cell is more than fifty units away horizontally; its object's, when the camera is at or
/// past the object's share distance horizontally; and otherwise its own, measured to its sort
/// centre. Both ends are in the same block frame, so the offsets are subtractions.
pub(super) fn placement_viewer_distance(
    p: &DegradePlacement,
    outdoors: bool,
    viewer: Vec3,
    share_2dsq: f32,
) -> (f32, Vec3) {
    p.object_origin
        .and_then(|o| {
            dereth_world_render::objects::parts::object_viewer_distance(
                o, outdoors, viewer, share_2dsq,
            )
        })
        .unwrap_or_else(|| {
            dereth_world_render::objects::parts::viewer_distance_and_heading(p.centre, viewer)
        })
}

/// For **one part**, returning
/// both the distance it handed `get_degrade` and the level that came back.
///
/// The distance is returned as well as the level so that
/// [`WorldScene::part_degrade_probe`] can publish the lookup's *input*: a test that only saw
/// the output could assert the selection agreed with itself and never that it agreed with the
/// record. This is [`dereth_world_render::objects::parts::select_level`]'s only implementation
/// path, and both callers go through it.
pub(super) fn part_level(
    pl: &PartLevels,
    part: &dereth_animation::parts::PhysicsPart,
    is_player: bool,
    cam: Vec3,
    shared: Option<(f32, Vec3)>,
    globals: &dereth_world_render::objects::degrade::DegradeGlobals,
) -> PartFrameChoice {
    use dereth_world_render::objects::degrade::DegradeMode;
    use dereth_world_render::objects::parts::{part_viewer_distance, select_level, PartDraw};
    let mut pd = PartDraw {
        pos: part.pos,
        draw_pos: part.pos,
        gfxobj_scale: part.gfxobj_scale,
        cypt: 0.0,
        deg_level: 0,
        deg_mode: DegradeMode::None,
        no_draw: part.no_draw(),
    };
    // Past the object's share distance the part is handed the object's distance and heading
    // (`shared`); inside it the part measures its own.
    let (cypt, heading) = part_viewer_distance(&pd, pl.sort_center, cam, shared);
    select_level(
        &mut pd,
        pl.info.as_deref(),
        is_player,
        cypt,
        heading,
        globals,
    );
    // `select_level`'s own distance over the z scale -- re-derived rather than returned, because
    // the function's contract is the level and the frame, and this is the probe's business.
    let z = part.gfxobj_scale.z;
    let dist = if z != 0.0 { cypt / z } else { cypt };
    // LINT-OK: a level index into a record whose longest shipped form has six levels.
    #[allow(clippy::cast_possible_truncation)]
    let level = pd.deg_level as u32;
    // `select_level` fills `draw_pos` -- it is
    // the draw-frame calculation's answer and the last line of the transcription -- and
    // dropping it leaves every creature's foliage and flame billboards facing
    // whichever way the animation frame left them. It is returned, with the mode beside
    // it so a probe can say *why* the frame moved.
    //
    // The fourth field, `cypt`, is the viewer distance the update above
    // computed — it is `select_level`'s first act to store it in `PartDraw::cypt` — and it is
    // the object pass's sort key. A struct rather than a tuple:
    // four positional floats and integers is one transposition away from a
    // silent defect.
    PartFrameChoice {
        cypt,
        heading,
        distance: dist,
        level,
        mode: pd.deg_mode,
        draw_pos: pd.draw_pos,
    }
}
