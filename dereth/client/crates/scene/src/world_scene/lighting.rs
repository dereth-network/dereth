//! Static and frame lighting, fog and environment colors.

use super::*;

/// Decode one `0x02……` setup's light entries in key order; a
/// bare `0x01……` graphics-object id has none.
/// The serialized light entry has no `type` field, so this path chooses point/type 0
/// and converts the packed colour word into float components.
pub(super) fn read_setup_lights(store: &RetailDatStore, id: DataId) -> Vec<LightInfo> {
    if dereth_dat::divine_type(id) != Some(DbType::Setup) {
        return Vec::new();
    }
    let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
        return Vec::new();
    };
    let Ok(setup) = dereth_assets::Setup::decode_payload_in(store.era_of(id), id, &bytes) else {
        return Vec::new();
    };
    setup
        .lights
        .values()
        .map(|l| LightInfo {
            light_type: LightType::Point,
            offset: l.frame,
            viewerspace_location: Vec3::ZERO,
            color: set_color32(l.color_argb),
            intensity: l.intensity,
            falloff: l.falloff,
            cone_angle: l.cone_angle,
        })
        .collect()
}

/// The same for a live object's `SetupData` (`dereth_animation`'s decode of the same record).
pub(super) fn setup_data_lights(setup: &dereth_animation::data::SetupData) -> Vec<LightInfo> {
    setup
        .lights
        .values()
        .map(|l| LightInfo {
            light_type: LightType::Point,
            offset: l.frame,
            viewerspace_location: Vec3::ZERO,
            color: set_color32(l.color_argb),
            intensity: l.intensity,
            falloff: l.falloff,
            cone_angle: l.cone_angle,
        })
        .collect()
}

/// Build the render batch for one cell's
/// meshes: every **static** light of the pool, transformed into
/// the mesh's space and accumulated per vertex with `calc_point_light`, then
/// `v.diffuse = 0xFF000000 | byte(r) << 16 | byte(g) << 8 | byte(b)`.
///
/// The mesh space here is the **block's** (the cell frame is folded into the vertices)
/// rather than the cell's, so the light is brought into block space by subtracting the block
/// origin instead of `globaltolocal(cell frame)`; the two are one rigid motion apart and the
/// arithmetic per vertex is the same function of the same distances. The alpha byte is left
/// as the bake stamped it; the client's burn forces it to `0xFF` and takes the alpha from
/// the default material's `Diffuse.a` (1.0) instead.
pub(super) fn burn_env_cell(cell: &mut EnvCellDraw, pools: &LightPools, origin: (f32, f32)) {
    let locals: Vec<LightInfo> = pools
        .statics
        .iter()
        .map(|l| {
            let mut i = l.info;
            i.offset.origin = Vec3::new(
                i.offset.origin.x - origin.0,
                i.offset.origin.y - origin.1,
                i.offset.origin.z,
            );
            i
        })
        .collect();
    for m in &mut cell.meshes {
        for v in m.vertices.as_chunks_mut::<OBJECT_VERTEX_STRIDE>().0 {
            let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
            let pos = Vec3::new(f(0), f(4), f(8));
            let normal = Vec3::new(f(12), f(16), f(20));
            let mut c = [0.0f32; 3];
            for li in &locals {
                if li.light_type == LightType::Point {
                    dereth_world_render::lighting::calc_point_light(pos, normal, li, &mut c);
                }
            }
            let o = OBJECT_DIFFUSE_OFFSET;
            // Little-endian `0xAARRGGBB`: B, G, R, A in memory.
            v[o] = burn_byte(c[2]);
            v[o + 1] = burn_byte(c[1]);
            v[o + 2] = burn_byte(c[0]);
        }
    }
}

/// The `byte(c)` of the burn: `c * 255` truncated to an integer, clamped to a byte.
pub(super) fn burn_byte(c: f32) -> u8 {
    // LINT-OK: clamped to 0..=255 before the cast. Not a float conversion by `as`.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    {
        dereth_primitives::num::to_i32(c * 255.0).clamp(0, 255) as u8
    }
}

/// As the shader sees it: the
/// enabled slots' `D3DLIGHT9`s into the per-draw block, with `D3DRS_LIGHTING` on and the
/// material's Emissive as the subset draw resolves it -- the surface's `luminosity`
/// when positive, else what the bound material carries. `emissive_from_vertex`
/// is the burned-in env-cell branch (emissive colour taken from the vertex).
pub(crate) fn bind_lights(
    world: &mut PerDrawConstants,
    lights: &[D3dLight],
    emissive: f32,
    emissive_from_vertex: bool,
) {
    let n = lights
        .len()
        .min(dereth_world_render::lighting::HARDWARE_LIGHT_SLOTS);
    // LINT-OK: at most eight. Not a float conversion of anything measured.
    #[allow(clippy::cast_precision_loss)]
    {
        world.lighting_params = [
            1.0,
            n as f32,
            emissive,
            if emissive_from_vertex { 1.0 } else { 0.0 },
        ];
    }
    for (k, l) in lights.iter().take(n).enumerate() {
        let p = if l.light_type == dereth_world_render::lighting::D3DLIGHT_DIRECTIONAL {
            l.direction
        } else {
            l.position
        };
        world.light_pos[k] = [p[0], p[1], p[2], l.range];
        // LINT-OK: a three-valued enum. Not a float conversion of anything measured.
        #[allow(clippy::cast_precision_loss)]
        {
            world.light_diffuse[k] = [
                l.diffuse[0],
                l.diffuse[1],
                l.diffuse[2],
                l.light_type as f32,
            ];
        }
    }
}

pub(super) fn blend_override_channel(base: u8, target: u8, transition: f32) -> u8 {
    // The client converts both bytes to extended precision, multiplies by the stored f32
    // transition, subtracts, and converts to an integer. Using f64 here retains the extended-precision
    // intermediate instead of rounding at each Rust
    // f32 operator; both operands and the stored transition still have their native widths.
    let base = f64::from(base);
    let target = f64::from(target);
    let value = base - (base - target) * f64::from(transition);
    let value = dereth_primitives::num::to_i32_f64(value).clamp(0, 255);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        value as u8
    }
}

pub(super) fn blend_override_scalar(base: f32, target: f32, transition: f32) -> f32 {
    // The client's extended-precision expression stores only its final result back to the f32 landscape field.
    #[allow(clippy::cast_possible_truncation)]
    {
        (f64::from(base) - (f64::from(base) - f64::from(target)) * f64::from(transition)) as f32
    }
}

pub(super) fn blend_override_color(base: u32, target: u32, transition: f32) -> u32 {
    let channel = |shift: u32| {
        u32::from(blend_override_channel(
            u8::try_from((base >> shift) & 0xFF_u32).unwrap_or(0),
            u8::try_from((target >> shift) & 0xFF_u32).unwrap_or(0),
            transition,
        )) << shift
    };
    channel(24) | channel(16) | channel(8) | channel(0)
}

pub(super) fn color_rgb(color: u32) -> [u8; 3] {
    [
        u8::try_from((color >> 16) & 0xFF).unwrap_or(0),
        u8::try_from((color >> 8) & 0xFF).unwrap_or(0),
        u8::try_from(color & 0xFF).unwrap_or(0),
    ]
}
impl SceneDraw {
    /// Update landscape lighting for the current time of day.
    ///
    /// ```text
    /// if (t, &ambLevel, &ambColor, &sunDir, &sunColor):
    ///     ambLevel = max(ambLevel, minimum_ambient)
    ///     (ambLevel, ambColor, sunDir, sunColor)
    /// ```
    ///
    /// **The sun direction is not normalised.** Its *length is the brightness*, and the
    /// landscape lighting's `n . sunlight`
    /// already carries the brightness. It is passed through untouched; normalising it flattens
    /// the whole day cycle.
    ///
    /// Returns true when the lighting changed, which is what makes the resident blocks re-bake:
    /// the landscape-lighting update ends by re-lighting every loaded landblock, and
    /// this crate reaches that through the window's own rebuild path.
    ///
    /// **One deliberate departure, and it is not observable.** The client re-lights every
    /// loaded block on *every* light tick without comparing; this compares first and skips the
    /// rebuild when the four values are bit-identical, which they are inside a flat stretch of
    /// the sky time-of-day ramp. `calc_lighting` is a pure function of the mesh and these four
    /// values, so the skipped rebuild would have produced the same vertices — and re-meshing a
    /// 49-block window to write the same bytes is a visible hitch for no pixels.
    pub(super) fn apply_lighting(&mut self, ws: &mut WorldState) -> bool {
        // The landscape lighting override has two arms. With always-daylight
        // set the function **discards** the caller's four arguments and re-asks the region
        // for the lighting at `0.5f`; with it clear it takes
        // what the time update computed for `present_time_of_day`. The caller clamps before
        // the override; only the always-daylight re-fetch repeats that clamp.
        // See
        // [`Self::set_always_daylight`] for why this one number is the whole option.
        let t = if ws.always_daylight {
            0.5
        } else {
            ws.clock.present_time_of_day
        };
        let Some(group) = dereth_world_render::sky::present_day_group(
            self.sky_region.as_deref().unwrap_or(&self.land.region),
            ws.clock.current_year,
            ws.clock.current_day,
        ) else {
            return false;
        };
        let l = dereth_world_render::sky::get_lighting(group, t);
        let mut want = LandscapeLighting {
            // Landscape minimum ambient is `0.2`, read from its static initializer.
            ambient_level: l.ambient_level.max(MIN_AMBIENT),
            ambient_color: l.ambient_color,
            sunlight: l.sun_vec,
            sunlight_color: l.sun_color,
        };
        let override_state = ws.environment_override.snapshot();
        if override_state.enabled {
            let transition = override_state.transition;
            // The per-frame update advances this blend before the landscape-lighting update.
            // Its always-daylight arm then discards
            // the blended ambient and re-asks the region for noon, but the shared transition
            // has still advanced. This ordering is why the increment is outside the guard.
            if !ws.always_daylight {
                if transition >= 1.0 {
                    want.ambient_level = override_state.ambient_level;
                    want.ambient_color = color_rgb(override_state.ambient_color);
                } else {
                    want.ambient_level = blend_override_scalar(
                        want.ambient_level,
                        override_state.ambient_level,
                        transition,
                    );
                    let base = (u32::from(want.ambient_color[0]) << 16)
                        | (u32::from(want.ambient_color[1]) << 8)
                        | u32::from(want.ambient_color[2]);
                    want.ambient_color = color_rgb(blend_override_color(
                        base,
                        override_state.ambient_color,
                        transition,
                    ));
                }
            }
            if transition < 1.0 {
                ws.environment_override.advance();
            }
        }
        if want == self.land.lighting {
            return false;
        }
        self.land.lighting = want;
        true
    }

    /// The landscape time update's fog tail:
    ///
    /// ```text
    /// mark fog user-disabled unless the fog option is on
    /// if user/world fog is disabled, return
    /// enable fixed-function fog
    /// if the world has no fog values at time t, return
    /// if the admin override is enabled, blend toward it
    /// write the fog color, minimum and maximum states
    /// ```
    ///
    /// The fog-state update writes `D3DRS_FOGCOLOR` (0x22, packed
    /// ARGB), `D3DRS_FOGSTART` (0x24) from the minimum, and `D3DRS_FOGEND` (0x25) from the maximum.
    /// The mode was fixed once at start-up and never changes: table `D3DFOG_NONE`,
    /// vertex `D3DFOG_LINEAR`, `D3DRS_RANGEFOGENABLE` 1 — see
    /// [`dereth_world_render::sky::linear_fog_factor`].
    ///
    /// Three things this does **not** do, each read rather than assumed:
    ///
    /// * **No draw-distance preference enters the range.** `Render.LandscapeDrawDistance`
    ///   does not scale it; the fog update reads only
    ///   the world's minimum and maximum fog distances.
    /// * **Indoors is not special.** System fog disablement is controlled only by the map
    ///   camera's two mode transitions. An environment cell carries no fog field and no cell turns
    ///   fog off; an interior is
    ///   drawn with the world's fog and simply has nothing far enough away to show it.
    /// * **Admin override only.** The visual environment command is the sole writer of the
    ///   override state; this reproduces that writer.
    ///
    /// Returns true when the fog moved, for symmetry with [`Self::apply_lighting`]; nothing
    /// has to be rebuilt when it does, because the fog is a per-frame constant and not a bake.
    pub(super) fn apply_fog(&mut self, ws: &mut WorldState) -> bool {
        let (want, advance_override) = self.computed_world_fog_state(ws);
        if advance_override {
            ws.environment_override.advance();
        }
        if want == self.fog {
            return false;
        }
        self.fog = want;
        true
    }

    /// The [`dereth_render::camera::FogParams`] the region asks for at the clock's current
    /// moment, before [`Self::apply_fog`] latches it.
    ///
    /// [`dereth_world_render::sky::get_world_fog`] answering `None` is "leave the fog alone", which on the device means the
    /// previous colour and range stay installed with `D3DRS_FOGENABLE` still on. It cannot
    /// happen in the shipped region — every sky time-of-day row of all twenty day groups has
    /// `world_fog = 1` — so the arm is reproduced as "keep what is installed" rather than
    /// being given an invented default.
    #[must_use]
    pub fn world_fog_state(&self, ws: &WorldState) -> dereth_render::camera::FogParams {
        self.computed_world_fog_state(ws).0
    }

    pub(super) fn computed_world_fog_state(
        &self,
        ws: &WorldState,
    ) -> (dereth_render::camera::FogParams, bool) {
        if !self.cfg.world_fog {
            return (
                dereth_render::camera::FogParams {
                    enabled: false,
                    ..self.fog
                },
                false,
            );
        }
        let Some(group) = dereth_world_render::sky::present_day_group(
            self.sky_region.as_deref().unwrap_or(&self.land.region),
            ws.clock.current_year,
            ws.clock.current_day,
        ) else {
            return (
                dereth_render::camera::FogParams {
                    enabled: true,
                    ..self.fog
                },
                false,
            );
        };
        let Some(f) = dereth_world_render::sky::get_world_fog(group, ws.clock.present_time_of_day)
        else {
            return (
                dereth_render::camera::FogParams {
                    enabled: true,
                    ..self.fog
                },
                false,
            );
        };
        let mut want = dereth_render::camera::FogParams {
            // The fog state's packed ARGB, with alpha 0xFF as the world-fog query forces it.
            color: 0xFF00_0000
                | (u32::from(f.color[0]) << 16)
                | (u32::from(f.color[1]) << 8)
                | u32::from(f.color[2]),
            near: f.min,
            far: f.max,
            enabled: true,
        };
        let mut advance_override = false;
        let override_state = ws.environment_override.snapshot();
        if override_state.enabled {
            let transition = override_state.transition;
            if transition >= 1.0 {
                want.color = override_state.fog_color;
                want.near = override_state.fog_min;
                want.far = override_state.fog_max;
            } else {
                want.color = blend_override_color(want.color, override_state.fog_color, transition);
                want.near = blend_override_scalar(want.near, override_state.fog_min, transition);
                want.far = blend_override_scalar(want.far, override_state.fog_max, transition);
                advance_override = true;
            }
        }
        (want, advance_override)
    }

    /// The device fog state as it stands, for the tests and the log line.
    #[must_use]
    pub fn world_fog(&self) -> dereth_render::camera::FogParams {
        self.fog
    }

    /// The sky update and the two timers around it.
    ///
    /// The sky updates **every frame** — it is called before the `tick_size` gate — while the
    /// lighting and the fog are behind `next_light_tick` / `next_tick`.
    pub(super) fn update_sky(&mut self, ws: &mut WorldState, dt: f32) {
        let (t, year, day) = (
            ws.clock.present_time_of_day,
            ws.clock.current_year,
            ws.clock.current_day,
        );
        if let Some(sky) = self.sky.as_mut() {
            // `dt` for the sky animation accumulator: the sky's UV scroll is `dt`-scaled and
            // needs the elapsed time to scale by.
            sky.use_time(
                self.sky_region.as_deref().unwrap_or(&self.land.region),
                year,
                day,
                t,
                dt,
            );
            self.stats.sky_objects = sky.live();
            self.stats.sky_stats = sky.stats;
        }
    }

    /// Re-bake every resident block's vertex lighting, which is the landscape-lighting update's
    /// tail.
    ///
    /// The client calls on each loaded block directly; this
    /// crate reaches the same place by asking [`Self::stream`] to regenerate each slot's mesh,
    /// because `LandContext::generate` already runs `bake_lighting` after every geometry
    /// rebuild and a block's baked objects survive a `SlotWork::Mesh`.
    pub(super) fn relight_blocks(&mut self, ws: &mut WorldState) {
        let w = ws.streamer.window.mid_width();
        for xi in 0..w {
            for yi in 0..w {
                if ws
                    .streamer
                    .window
                    .slot(xi, yi)
                    .and_then(|s| s.mesh.as_ref())
                    .and_then(SlotMesh::get::<LandblockMesh>)
                    .is_none()
                {
                    continue;
                }
                self.want(ws, xi, yi, SlotWork::Mesh);
            }
        }
    }

    /// The landscape time update's environment and lighting tick pair.
    ///
    /// The clock update runs every frame, before the gate. Once `cur_time >= next_tick`,
    /// `next_tick` becomes `cur_time` plus the sky's tick size; if also
    /// `cur_time > next_light_tick`, the lighting is read and applied to the landscape and
    /// `next_light_tick` becomes `cur_time` plus the sky's light tick size; then the fog is
    /// updated.
    ///
    /// The fog arm is reproduced through [`Self::apply_fog`] and the renderer's
    /// linear-fog constants, with the shared admin override transition applied in the same
    /// consumer.
    pub(super) fn use_time_sky(&mut self, ws: &mut WorldState, now: f64, dt: f32) {
        world_step::advance_clock(ws, now);
        self.update_sky(ws, dt);
        let Some(light_tick) = world_step::tick_schedule(
            ws,
            self.sky_region.as_deref().unwrap_or(&self.land.region),
            now,
        ) else {
            return;
        };
        if light_tick && self.apply_lighting(ws) {
            self.relight_blocks(ws);
        }
        // The fog is on the *outer* tick, not the light tick:
        // falls through the `next_light_tick` branch into fog application every `tick_size`.
        self.apply_fog(ws);
    }

    /// The landscape weather-enable flag, whose default is 1. A test
    /// needs it because the one honest way to say "these pixels are the weather layer" is to
    /// draw the same frame with the layer suppressed: sky-object creation makes
    /// no object at all for a `properties & 4` entry while it is clear, so the differential is
    /// exactly the weather.
    ///
    /// The stored weather-enable byte has only two
    /// callers: initial scene setup and the character-option handler, both passing
    /// the inverse of the `DisableMostWeatherEffects` option. Here
    /// `OptionSideEffect::EnableWeather` names the call; see
    /// the client shell's `App::apply_player_option_effects`.
    pub fn set_weather_enabled(&mut self, on: bool) {
        if let Some(sky) = self.sky.as_mut() {
            sky.weather_enabled = on;
        }
    }

    /// The landscape weather-enable flag as it stands — the read side of
    /// [`Self::set_weather_enabled`], so a station can say *"the weather layer is off"* rather
    /// than *"a function was called"*. `None` before the sky exists.
    #[must_use]
    pub fn weather_enabled(&self) -> Option<bool> {
        self.sky.as_ref().map(|s| s.weather_enabled)
    }

    /// The landscape fog-enable flag is the `DisableDistanceFog` character option's whole
    /// effect.
    ///
    /// Initial option setup and option-change handling both compute the inverse of the
    /// `DisableDistanceFog` option.
    /// The result is stored straight into the landscape flag.
    /// There is no third state and no deferred option transaction.
    /// The landscape tick reads that same byte.
    /// When it is clear, the tick disables fixed-function fog.
    /// It then skips world-fog lookup and fog-property installation.
    /// When it is set, those normal fog steps remain enabled.
    /// That is why this setter changes the existing `SceneConfig::world_fog` consumer rather
    /// than adding a separate render switch, and it
    /// skips the whole world-fog query and state update when it is clear, which is
    /// what [`Self::world_fog_state`] already models off `SceneConfig::world_fog`. This is the
    /// setter that lets the character option reach a field that is otherwise a start-up
    /// switch.
    ///
    /// [`Self::apply_fog`] is re-run here rather than waiting for the next landscape tick,
    /// because retail's immediate tick reset does not exist on this path and a
    /// player who unticks the row expects the fog to go within the frame.
    pub fn set_world_fog(&mut self, ws: &mut WorldState, on: bool) -> bool {
        if self.cfg.world_fog == on {
            return false;
        }
        self.cfg.world_fog = on;
        self.sync_environment_override_flags(ws);
        self.apply_fog(ws)
    }

    /// Attach the process-owned environment-override globals to a newly constructed landscape.
    /// The handle includes the current transition, so this never restarts an in-flight blend.
    pub(crate) fn set_environment_override_state(
        &mut self,
        ws: &mut WorldState,
        state: EnvironmentOverrideState,
    ) {
        ws.environment_override = state;
        self.sync_environment_override_flags(ws);
    }

    /// Refresh the two sky-pass predicates after the shared Admin state changes.
    pub(crate) fn sync_environment_override_flags(&mut self, ws: &mut WorldState) {
        let enabled = ws.environment_override.snapshot().enabled;
        if let Some(sky) = self.sky.as_mut() {
            sky.override_enabled = enabled;
            sky.fog_enabled = enabled && self.cfg.world_fog;
        }
    }

    /// The always-daylight flag is the `PersistentAtDay` character option's
    /// whole effect.
    ///
    /// In the original client, enabling it stores a normalized boolean.
    /// The operation then clears the low and high halves of the next general landscape tick
    /// and the low and high halves of the next lighting tick.
    /// Those are its five stores: one flag byte and four timer words.
    /// It replaces no sky object,
    /// enables or disables no weather object,
    /// and changes neither the game clock
    /// nor the current day and year.
    /// Existing landscape colors remain until lighting is evaluated again;
    /// clearing both timers makes that evaluation happen on the next frame.
    /// The ordinary arm continues to use the caller's current time-of-day lighting,
    /// while the always-daylight arm ignores those four lighting values
    /// and queries the region again at time of day `0.5`.
    /// Both arms then apply the same minimum-ambient clamp,
    /// so the option affects landscape lighting only.
    /// Its observable value is noon lighting, not a frozen world clock;
    /// disabling it returns the next evaluation to the live time of day.
    /// The timer reset makes either transition visible without waiting for a scheduled tick.
    ///
    /// In other words, the flag plus a forced re-tick makes the change visible on the next frame rather
    /// than at the next 0.5 s landscape tick. The flag has exactly **two** readers: the `/day`
    /// console command, which toggles it, and landscape lighting, whose always-daylight arm
    /// **throws away the caller's four arguments** and re-asks the region for the lighting at
    /// time-of-day `0.5`, followed by the same minimum-ambient clamp as the normal arm.
    ///
    /// So *Always Day* is **noon landscape lighting**, and nothing else: it does not touch the
    /// sky dome, weather or clock. This Rust setter stores `always_daylight` and immediately
    /// calls [`Self::apply_lighting`], rather than writing the original client's timer words;
    /// the immediate update is equivalent to forcing the next lighting evaluation.
    pub fn set_always_daylight(&mut self, ws: &mut WorldState, on: bool) -> bool {
        if ws.always_daylight == on {
            return false;
        }
        ws.always_daylight = on;
        // Equivalent to the original client's timer reset: re-light now rather than waiting
        // for the next landscape tick -- and, as that tick does, re-light the resident blocks
        // too. The landscape's lighting is baked into each block's vertex colours when the
        // block is built, so storing the new values alone re-lit only blocks built after the
        // switch; and because the stored values then already matched, the next tick found
        // nothing to do either, and the blocks on screen kept the old hour's light for good.
        let changed = self.apply_lighting(ws);
        if changed {
            self.relight_blocks(ws);
        }
        changed
    }

    /// The baked vertex colours of the block the viewer stands in, for the tests: what the
    /// landscape lighting has actually been applied to, as opposed to what it is set to.
    #[must_use]
    pub fn viewer_block_colours(&self, ws: &WorldState) -> Option<Vec<[u8; 3]>> {
        let r = ws.streamer.window.mid_radius();
        ws.streamer
            .window
            .slot(r, r)?
            .mesh
            .as_ref()
            .and_then(SlotMesh::get::<LandblockMesh>)
            .map(|m| m.colours.clone())
    }

    /// Return `sunlight` and `ambient_level` as they stand, for the tests. The vector
    /// is unnormalised and its length is the brightness.
    #[must_use]
    pub fn landscape_lighting(&self) -> LandscapeLighting {
        self.land.lighting
    }

    // The per-object physics / animation step lives in
    // [`dereth_client_runtime::object_step`]: `refresh_object_env` .. `pull_stuck_objects` and the
    // `server_object_*` motion probes. The scene keeps the render half of a `SceneObject`
    // and hands the step the simulation half; these are the call sites, and the four
    // counters the step keeps come back as an `ObjectStepStats` folded into `SceneStats`.

    pub(super) fn fold_step_stats(
        &mut self,
        d: dereth_client_runtime::object_step::ObjectStepStats,
    ) {
        world_step::StepStatsSink::fold_steps(&mut self.stats, d);
    }

    /// The tail of the camera update — **the body fades as the chase camera
    /// closes on it.**
    ///
    /// * With the camera in the head, the latched value becomes 1.0 and the player's whole
    ///   hierarchy is set to translucency 1.0.
    /// * With no pivot position, a latched value above 0 is reset to 0 and applied.
    /// * With the pivot within 0.45 of the viewer, the value is
    ///   `clamp(1 - (0.2 - d) / -0.24999999, 0, 1)` and is applied to the whole hierarchy.
    /// * Otherwise a latched value above 0 is reset to 0 and applied.
    ///
    /// [`dereth_client_runtime::camera::CameraControl::player_translucency`] is that value; applying it is the
    /// whole of the wire.
    ///
    /// **The latch is load-bearing, not an optimisation.** The client only issues the `0.0`
    /// call on the *transition*, and clears the
    /// `NoDraw` bit on every call with `t != 1.0`. Issuing `0.0` unconditionally would
    /// therefore make the chase camera un-hide any part an animation's `NoDraw` hook had
    /// hidden, every frame. So: apply whenever the value is non-zero, and apply a zero only
    /// once, exactly as the three branches above do.
    ///
    /// **Hierarchical** means the operation
    /// recurses into `children` — so a wielded weapon fades with the hand holding it. The
    /// player's children here are the held objects attached to him.
    pub fn apply_camera_translucency(&mut self, ws: &mut WorldState) {
        world_step::apply_camera_translucency(ws, &self.cfg);
    }

    /// The landscape half of the position change `(pos, 1)` during player teleport.
    ///
    /// After the position change's same-cell-and-live-cell fast path, the teleport arm calls
    /// whole-landscape release regardless of how near the destination
    /// is. A normal [`Self::recenter`] deliberately retains the blocks shared by the old and
    /// new windows, so it cannot stand in for that edge. Move every resident draw through the
    /// existing release queues, hand back every visible cell, and recreate the same-radius
    /// window at the position the accepted packet named.
    /// [`Self::stream`] performs the device-resource release and full arrivals later in this
    /// frame, after [`Self::sync_objects`] has run the queued
    /// half.
    pub(crate) fn release_landscape_for_teleport(
        &mut self,
        ws: &mut WorldState,
        destination: dereth_primitives::LandblockId,
    ) {
        let departed: Vec<_> = self.blocks.keys().copied().collect();
        self.pending.clear();
        for block in &departed {
            if let Some(draw) = self.blocks.remove(block) {
                self.released_blocks.push(draw);
            }
        }
        self.release_block_interiors(ws, &departed);
        self.static_pool_key = None;

        let radius = ws.streamer.window.mid_radius();
        let was = ws.streamer.window.viewer_block();
        ws.streamer.window = LandblockWindow::new(radius);
        let viewer = (i32::from(destination.x()), i32::from(destination.y()));
        if let Some(was) = was {
            world_step::rebase_render_space_particles(ws, (viewer.0 - was.0, viewer.1 - was.1));
        }
        let actions = ws.streamer.window.update_block(viewer);
        self.queue(ws, &actions);
    }

    /// The frame's light pools and the env-cell burn-in.
    ///
    /// * On a cell change, clear the static-light count and rebuild the static pool from every
    ///   visible cell's light records whose `state & 1` is set. The current degrade level sets
    ///   the static and dynamic pool caps.
    /// * On every frame, clear the dynamic-light count, add the viewer light, then add every
    ///   non-static light record of the visible environment cells. A light belongs to a cell
    ///   only after light registration, so an object standing in a land cell, or held after
    ///   `leave_world`, contributes nothing.
    /// * Environment-cell drawing updates static vertex colors for every cell whose burn is
    ///   not at the current static-light count.
    pub(super) fn update_world_lights(&mut self, ws: &mut WorldState) {
        if !self.cfg.object_lighting {
            return;
        }
        let (cell, player) = self.player_origin(ws);
        let keys: Vec<(i32, i32)> = self.blocks.keys().copied().collect();
        let key = (cell, keys);
        if self.static_pool_key.as_ref() != Some(&key) {
            let level = self.degrade.governor.level();
            self.light_pools.max_static = level.max_static_lights;
            self.light_pools.max_dynamic = level.max_dynamic_lights;
            self.light_pools.clear_statics();
            for block in self.blocks.values() {
                for l in block.cell_lights.iter().filter(|l| l.is_static) {
                    // The offset from the player block to the light cell, plus `frame.origin`,
                    // gives the object's frame in the player's space, which
                    // in this build is render space -- the block origin folded in.
                    let frame = Frame::new(
                        Vec3::new(
                            l.frame.origin.x + block.origin.0,
                            l.frame.origin.y + block.origin.1,
                            l.frame.origin.z,
                        ),
                        l.frame.rotation,
                    );
                    self.light_pools
                        .add_static_from(&l.info, l.cell, &frame, player.origin);
                }
            }
            self.static_pool_key = Some(key);
        }
        self.light_pools.clear_dynamics();
        let has_player = ws.character.is_some();
        if self.viewer_light {
            self.light_pools.add_dynamic_from(
                &viewer_light(has_player),
                CellId(cell),
                &player,
                player.origin,
            );
        }
        for o in ws.objects.values() {
            if !o.drawn {
                continue;
            }
            let Some(pos) = o.sim.position else { continue };
            if dereth_physics::landdefs::is_outdoors(pos.cell) {
                continue;
            }
            if !o.sim.state.is_lighting_on() || o.sim.state.is_static() {
                continue;
            }
            let driver = o.sim.driver.borrow();
            let Some(setup) = driver.part_array.setup.as_ref() else {
                continue;
            };
            for info in setup_data_lights(setup) {
                self.light_pools
                    .add_dynamic_from(&info, pos.cell, &o.frame, player.origin);
            }
        }
        if let Some(c) = ws.character.as_ref() {
            let pos = c.position();
            if !dereth_physics::landdefs::is_outdoors(pos.cell)
                && ws.character_state.is_lighting_on()
            {
                let frame = self.render_frame_of(ws, pos);
                let driver = c.driver();
                if let Some(setup) = driver.part_array.setup.as_ref() {
                    for info in setup_data_lights(setup) {
                        self.light_pools
                            .add_dynamic_from(&info, pos.cell, &frame, player.origin);
                    }
                }
            }
        }
        let count = self.light_pools.statics.len();
        let pools = &self.light_pools;
        for block in self.blocks.values_mut() {
            let origin = block.origin;
            for cell in &mut block.env_cells {
                if cell.burned_count == Some(count) {
                    continue;
                }
                burn_env_cell(cell, pools, origin);
                cell.burned_count = Some(count);
            }
        }
    }

    /// The scene's sunlight description, as the tail of
    /// the light update builds it from the landscape's sun direction and
    /// `sunlight_color` (which the normal-mode render and the position change copy into
    /// the world light list for an outdoor or `seen_outside` viewer). `None` while the sun vector is
    /// below the `0.0002` gate, which is "a light that contributes nothing".
    #[must_use]
    pub fn sun_light(&self) -> Option<D3dLight> {
        let l = &self.land.lighting;
        let c = [
            f32::from(l.sunlight_color[0]) * BYTE_TO_FLOAT,
            f32::from(l.sunlight_color[1]) * BYTE_TO_FLOAT,
            f32::from(l.sunlight_color[2]) * BYTE_TO_FLOAT,
        ];
        sunlight_light(l.sunlight, c)
    }

    /// World-light ambient color follows the two position-change arms: an
    /// outdoor or `seen_outside` viewer gets
    /// the landscape's calculated object-light level and ambient color;
    /// any other interior gets world ambient light 0.2 in colour `0xFFFFFFFF`.
    /// Landscape-lighting setup re-issues the
    /// first on every light tick, which cannot run for a viewer the landscape released.
    #[must_use]
    pub fn world_ambient_color(&self, ws: &WorldState) -> [f32; 3] {
        let indoors_unseen = ws.viewer_cell().is_some_and(|c| !ws.cell_seen_outside(c));
        if indoors_unseen {
            return world_ambient(INDOOR_AMBIENT_LEVEL, 0xFFFF_FFFF);
        }
        let l = &self.land.lighting;
        let c = (u32::from(l.ambient_color[0]) << 16)
            | (u32::from(l.ambient_color[1]) << 8)
            | u32::from(l.ambient_color[2]);
        world_ambient(calc_object_light(l.sunlight, l.ambient_level), c)
    }

    /// The `D3DLIGHT9`s one object mesh is drawn with:
    /// sunlight use set to 1 (the sun alone) for the landscape's outdoor objects, and
    /// every dynamic light in distance order
    /// whose falloff sphere reaches the object's drawing sphere, then the statics, eight at
    /// most -- for everything the inner mesh draw draws with sunlight use 0.
    /// `centre`/`radius` are the drawing sphere in render space. Empty with the lighting
    /// switch off.
    #[must_use]
    pub fn object_light_set(&self, centre: Vec3, radius: f32, outdoors: bool) -> Vec<D3dLight> {
        if !self.cfg.object_lighting {
            return Vec::new();
        }
        let sun = self.sun_light();
        let active = if outdoors {
            let mut a = ActiveLights::default();
            use_sunlight_set(&mut a, true);
            a
        } else {
            minimize_object_lighting(&self.light_pools, centre, radius)
        };
        enabled_lights(&active, &self.light_pools, sun.as_ref())
    }

    /// The light set for a cell mesh:
    /// every dynamic light; the statics are in the vertices.
    #[must_use]
    pub fn envcell_light_set(&self) -> Vec<D3dLight> {
        if !self.cfg.object_lighting {
            return Vec::new();
        }
        let active = minimize_envcell_lighting(&self.light_pools);
        enabled_lights(&active, &self.light_pools, None)
    }

    /// The light pools as they stand, for the tests.
    #[must_use]
    pub fn light_pools(&self) -> &LightPools {
        &self.light_pools
    }

    /// How many light objects the resident blocks' interior cells registered.
    #[must_use]
    pub fn cell_light_objects(&self) -> usize {
        self.blocks.values().map(|b| b.cell_lights.len()).sum()
    }

    /// One interior cell's vertices as they will be uploaded: block-local
    /// position, block-local normal, and the burned `D3DFVF_DIFFUSE` RGB. A probe for the
    /// tests; empty for a cell no resident block holds.
    #[must_use]
    pub fn env_cell_burned_vertices(&self, cell: CellId) -> Vec<(Vec3, Vec3, [u8; 3])> {
        let mut out = Vec::new();
        for block in self.blocks.values() {
            for c in block.env_cells.iter().filter(|c| c.id == cell) {
                for m in &c.meshes {
                    for v in m.vertices.as_chunks::<OBJECT_VERTEX_STRIDE>().0 {
                        let f = |o: usize| f32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]);
                        let o = OBJECT_DIFFUSE_OFFSET;
                        out.push((
                            Vec3::new(f(0), f(4), f(8)),
                            Vec3::new(f(12), f(16), f(20)),
                            [v[o + 2], v[o + 1], v[o]],
                        ));
                    }
                }
            }
        }
        out
    }

    /// The block origin a cell's block-local frames are measured from, for
    /// the tests; `None` for a cell no resident block holds.
    #[must_use]
    pub fn cell_block_origin(&self, cell: CellId) -> Option<(f32, f32)> {
        self.blocks
            .values()
            .find(|b| b.env_cells.iter().any(|c| c.id == cell))
            .map(|b| b.origin)
    }
}
