//! What the drawn world is built from: `SceneConfig`, one switch per command-line option.
//!
//! Plain data. The presentation that builds the world reads it, and the application keeps the
//! one it built from so the world can be built again on the next world entry.

use dereth_primitives::CellId;

use crate::landblock::DEFAULT_LANDBLOCK;

/// What the scene is built from. Every field has a command-line switch; the defaults put a
/// fixed camera over Holtburg, which is what the headless capture regression-tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneConfig {
    /// The landblock the camera starts in, and the centre of the window.
    pub landblock: u16,
    /// An interior cell of [`Self::landblock`] to stand the offline body in, found
    /// through the cell's own `cell_bsp` so that its containment test accepts the point.
    /// `--start-cell`. Ignored when the server says where the
    /// player is, which is every connected run.
    pub start_cell: Option<CellId>,
    /// Give the interior cells' baked objects — a dungeon's tables, chairs,
    /// bookshelves and braziers — their bodies and their triangles. Always on in the client
    /// without a runtime switch; this switch exists for
    /// exactly the reason [`Self::building_portals`]' does, because the only honest evidence
    /// that the furniture is on screen and in the way is a differential against the same
    /// frame and the same walk without it.
    pub cell_statics: bool,
    /// Load each setup's **parts** and their graphics-object physics BSPs, so that
    /// object collision tests can traverse every part's own tree. Always on in the
    /// client: the presence of a physics BSP in the parts selects that collision branch.
    /// The switch exists for the reason [`Self::cell_statics`]'
    /// does — clearing it loads no parts, so the scan answers no and every object falls to its
    /// spheres, and that is the control the differential needs. `--no-mesh-collision`.
    pub mesh_collision: bool,
    /// Skip a static part when its selected detail level has no geometry.
    /// Eleven graphics objects have such levels in their degrade records.
    /// Always on in the client: viewer-distance selection runs
    /// on every part of every frame. The switch exists for the reason [`Self::cell_statics`]'
    /// does: clearing it draws those parts anyway, and that is the control the
    /// differential needs. See [`crate::models::draws_at_near_band`].
    pub part_degrades: bool,
    /// Radius of the meshed landblock window: `mid_width = 2r+1` blocks are meshed.
    pub land_radius: u32,
    /// How many rings of blocks grow scenery, buildings and static objects. Terrain is cheap
    /// and re-emitted per frame; objects are baked into world-space batches, so this is the
    /// knob that costs memory.
    pub scenery_radius: u32,
    /// How long the drawn world's `stream` may spend building landblocks in one frame. `None` builds
    /// everything queued before returning, which is what the headless captures and the tests
    /// rely on; the connected client sets a budget so that a teleport or login keeps drawing
    /// the portal tunnel while the window fills in, nearest blocks first, over later frames.
    pub stream_budget: Option<std::time::Duration>,
    /// Build the landscape's per-cell composite textures with the device's compute shader rather
    /// than on the CPU, where the device can. The two are bit-identical; this only moves the work.
    /// The client clears it for `[Render] TerrainBlending = cpu`.
    pub gpu_terrain_merge: bool,
    /// Draw the landscape by blending each cell's terrain layers in the pixel shader instead of
    /// sampling a per-cell composite. Not retail's method and not bit-identical to it, but it
    /// keeps no composites in video memory: about a gigabyte of them at the largest draw distance
    /// and texture detail. Off here, so the tests draw retail's composites; the client sets it
    /// from `[Render] TerrainBlending`.
    pub terrain_splat: bool,
    /// The fourteen `Render.*` preferences and the three rendering degrade values.
    ///
    /// It is on the config rather than on the scene because the drawn world's `cfg` is public and
    /// the options page has to be able to change these between frames -- which is exactly the
    /// shape of the client's own poll: it re-reads the rendering preferences every frame
    /// and acts only on what changed.
    pub render: crate::render_prefs::RenderPreferences,
    /// Metres above the block's highest vertex the default camera sits. Only used without a
    /// body; with one the camera follows it.
    pub camera_height: f32,
    /// Put a character in the scene. `--no-character` turns it off, which gives a free camera
    /// over the static scene.
    pub character: bool,
    /// Where in the in-game day the session starts, `[0, 1)`. `None` runs the clock
    /// alone, which is what the client does with `GameTime.TimeZeroDelta`
    /// at its default 0.
    pub time_of_day: Option<f32>,
    /// Where in the Dereth *calendar* the session starts, as the absolute
    /// second a `TimeSync` would have put in. Overrides
    /// [`Self::time_of_day`], because it decides the hour too.
    ///
    /// The day fraction is not the whole environment: the sky calendar hashes
    /// `(current_year * days_per_year + current_day)` into one of twenty day groups,
    /// which carry different time-of-day colour ramps at the same hour. A
    /// headless scene asked for fraction 0.1814 through `time_of_day` lands on year 10 day 1,
    /// group 5 ("Sunny"), whose Foredawn `amb_color` is (177, 177, 255) — green equal to red.
    /// A recorded retail capture was taken at absolute second 303_449_042, which is
    /// year 120 day 223, group 15 ("Rainy"), `amb_color` (191, 134, 255). That difference,
    /// and not the interpolation, is the "night colour cast" measured against it.
    ///
    /// See the sky clock's `set_game_time`.
    pub game_time: Option<f64>,
    /// World fog is enabled by default. Clearing this disables fog and skips
    /// both the world-fog lookup and the update of its rendering properties.
    /// It exists because the only honest evidence that the far
    /// terrain is fogged is the same frame with the fog off.
    pub world_fog: bool,
    /// The three `Camera.*` preferences, applied to the body's camera when it is attached.
    /// See [`crate::camera::CameraPreferences`].
    pub camera: crate::camera::CameraPreferences,
    /// The four `Input.*` mouse-look preferences, applied with them.
    pub mouse_look: crate::actions::camera::MouseLookPreferences,
    /// Run and draw the particle emitters. Always on in the client; the switch
    /// exists because the only honest evidence that particles are on screen is a differential
    /// against the same frame with them off, and that differential has to come from the app's
    /// own update path rather than from a second, differently-built scene.
    pub particles: bool,
    /// Run the outdoor half of the portal machinery, so that a building's interior
    /// is visible through its windows and open doors. Always on in the client; the switch
    /// exists for exactly the reason [`Self::particles`]' does — the only honest evidence that
    /// an interior is on screen is a differential against the same frame without the pass, and
    /// "some pixels changed" is not that evidence unless the changed pixels are inside the
    /// portal's own projected outline and nowhere else.
    pub building_portals: bool,
    /// Clip each portal in screen space and copy its clipped view instead of
    /// treating every portal as visible.
    ///
    /// Always on in the client, which has no switch for it; clearing it makes every clip test
    /// answer "visible" and every view copy succeed, which is the control the differential
    /// needs. The measured cost of that difference was **3 of 20,504 changed pixels**, all
    /// within 6 px of an opening — so this is a small, local and *stated* difference, not a
    /// large one.
    pub portal_clip: bool,
    /// Draw the `mode == 1` **portal depth stamp**,
    /// with mask constant 7: a pre-transformed quad at the constant device depth `0.999999`,
    /// depth compare `ALWAYS`, no colour.
    ///
    /// Always on in the client. Clearing it omits the stamp; that is mostly invisible
    /// **only** because the interiors are drawn before the block's opaque batches and before
    /// every nearer block, so the omission is masked by draw order rather than by correctness.
    /// The control exists so a test can show the difference where the draw order does *not*
    /// mask it.
    pub portal_depth_stamp: bool,
    /// Flush queued translucent geometry **before** each building's portal pass.
    ///
    /// Always on in the client, which has no switch. Clearing it defers every translucent
    /// batch to one pass after the whole landscape, which is the control the differential
    /// needs.
    pub portal_alpha_flush: bool,
    /// Run the adaptive-degrade feedback loop,
    /// so that the degrade multiplier follows the measured frame rate instead of staying pinned.
    ///
    /// **The client runs it by default, as retail does**: [`crate::config::Config::scene_config`]
    /// takes it from the player's `Render.AutomaticDegrades` preference, registered true. This
    /// struct's own default is *off*, because it is the one subsystem whose output is a function of
    /// how fast the machine is, and `dereth_world_render::degrade_loop`'s own header states the
    /// standing decision that follows from it: "`DegradeGovernor::pinned` is what every test and
    /// every golden image must use". A scene a test builds directly therefore asks for the loop
    /// the way it asks for [`Self::particles`] or [`Self::building_portals`].
    pub auto_degrades: bool,
    /// Write the surface alpha into each vertex's diffuse colour.
    /// It is `(int)((1 - translucency) * 255)` for a `TRANSLUCENT` surface and
    /// `0xFF` for every other surface, in both polygon emission and vertex copying.
    ///
    /// Always on in the client, which has no switch: surface setup returns the byte and the
    /// caller writes it. Clearing it makes every vertex `0xFFFFFFFF`, which is also, byte for
    /// byte, the frame the same part would produce at `translucency 0`. That is the control the differential needs, and it is why the paired
    /// frame is a paired *translucency* rather than two different objects.
    pub surface_translucency: bool,
    /// Draw a part that carries a cloned material through that material's
    /// alpha, which is `1 - t` for translucency `t`.
    ///
    /// A different channel from [`Self::surface_translucency`], and deliberately a separate
    /// switch: that one is the surface's dat-authored `TRANSLUCENT` alpha; this one is
    /// the object's runtime override, driven by the chase camera's body fade,
    /// an animation's transparency hook or the server's translucency update.
    ///
    /// Always on in the client, which has no switch. Clearing it ignores the material alpha,
    /// which is byte for byte the frame the same parts produce with no material at all; that
    /// is the control the differential needs.
    pub material_translucency: bool,
    /// Light objects and interior cells with the fixed-function lights: the
    /// sun outdoors, the light pool's per-object selection indoors, the burned static light
    /// on every environment-cell vertex and the world's ambient light.
    ///
    /// Always on in the client: fixed-function lighting is enabled for
    /// every mesh subset. Clearing it leaves every object vertex white with no lights bound,
    /// which is the control the differentials need.
    pub object_lighting: bool,
    /// Hold **every** detail level of a baked placement and pick one
    /// per frame instead of baking
    /// the near-band answer once.
    ///
    /// Always on in the client: viewer-distance selection runs on every
    /// part of every frame. Clearing it gives one bake, the
    /// level selected at `d = 0`, at every distance and in every camera mode. That
    /// is the control the differential needs, and it is a **separate** switch from
    /// [`Self::part_degrades`] because clearing that one also draws the eleven designer
    /// markers.
    ///
    /// **`part_degrades` gates this one**, and that ordering is deliberate: clearing
    /// `part_degrades` means "do not consult the degrade record at all", which cannot
    /// coherently leave the per-frame level selection running. A test whose subject is the
    /// *creature* half of the guard therefore has to pin this to `false` in **both** arms, or
    /// it varies the whole landscape's level of detail as well.
    pub degrade_levels: bool,
    /// The same for everything that **moves** — creatures, server objects and the
    /// local body, whose parts use the same viewer-distance selection as static parts.
    ///
    /// [`Self::degrade_levels`] is the *baked* half and this is the *part-array* half. They
    /// are separate switches because they are separate mechanisms: a static's levels live in
    /// one vertex pool per block reassembled when a level changes, while a part is already
    /// drawn on its own with its own `PerDrawConstants`, so the level is a mesh index and
    /// nothing has to be reassembled. Clearing this bakes every part at the near band, the
    /// level selected at `d = 0`, at every distance.
    ///
    /// **`part_degrades` gates this one too**, for the reason `degrade_levels`' documentation
    /// gives: "do not consult the degrade record at all" cannot coherently leave a per-frame
    /// level selection running. A test whose subject is the near-band draw guard must pin
    /// this to `false` in **both** arms or it varies the meshes as well.
    pub part_degrade_levels: bool,
    /// Apply the billboard draw transform to a **baked static** whose
    /// selected level asks for one of the four billboarding modes, instead of computing the
    /// mode every frame and dropping it.
    ///
    /// Always on in the client: each part's draw transform is computed every frame and
    /// submitted in place of its unmodified position. Clearing
    /// it bakes every level into world space with the mode
    /// computed and discarded, so mode-2 and mode-5 cards face whichever way the dat placed
    /// them. That is the control the differential needs.
    ///
    /// **[`Self::degrade_levels`] gates this one**, for the same reason `part_degrades` gates
    /// that: the mode arrives *with* the selected level, so "do not pick
    /// a level per frame" cannot coherently leave a per-frame billboard running.
    pub static_billboards: bool,
    /// The same for everything that **moves**.
    /// The client draws each part at the transform derived from its selected level's
    /// billboard mode; submitting the unmodified position instead means a creature's mode-2
    /// and mode-5 parts never turn. **188 of 3,234** selected part-levels ask for a mode other
    /// than 1.
    ///
    /// [`Self::static_billboards`] is the *baked* half and this is the *part-array* half, the
    /// same split as [`Self::degrade_levels`] / [`Self::part_degrade_levels`] — and for the
    /// same reason: a baked static needs a re-transform as it is assembled, while a part is
    /// already drawn with its own `PerDrawConstants` and only needs a different frame in it.
    ///
    /// **[`Self::part_degrade_levels`] gates this one**, because the mode arrives with the
    /// selected detail level.
    pub part_billboards: bool,
    /// Defer a moving object's non-opaque subsets to
    /// `dereth_world_render::objects::alpha::AlphaLists` and drain them after the whole object
    /// pass — the clip list first, then the blend list, each in insertion order — instead of
    /// drawing every subset in place.
    ///
    /// Always on in the client: the alpha-delay mask is `0x0E`, so **every** non-opaque
    /// subset is queued. Clearing it draws every subset in place, in part order, which is the
    /// control the differential needs.
    pub part_alpha_lists: bool,
    /// Sort moving parts by descending camera depth,
    /// farthest first, before submitting them.
    ///
    /// Always on in the client, which sorts a cell's shadow-part list in the block's pass A
    /// before pass B draws it. **This build's version is a scene-wide sort rather than a
    /// per-cell one**, because this build has no per-cell shadow-part registration: it draws every
    /// moving object in one pass after the landscape (a standing deviation) instead
    /// of drawing each object inside the sort cell it overlaps. Within one cell the two orders
    /// agree; across cells the client gets far-to-near from the *cell* order and this gets it
    /// from the parts themselves, which is the same direction. Stated as a deviation rather
    /// than presented as the client's behaviour.
    ///
    /// Clearing it submits in `BTreeMap<ObjectId, _>` order, i.e. object-id order, then part
    /// order — which is the control the differential needs.
    pub part_depth_sort: bool,
    /// Release a block's visible interior cells and their static objects
    /// when it scrolls off the window,
    /// instead of leaving its interior cells, their walls and their furniture resident for the
    /// life of the session.
    ///
    /// Always on in the client: every block pushed off an edge is released unconditionally.
    /// Clearing it keeps the interiors resident — the block's
    /// draw data is dropped by the same `retain` either way, and nothing else is — which is
    /// the control the leak differential needs, and the only honest way to state a leak is
    /// against the same walk in the same binary with the release off.
    pub release_interiors: bool,
    /// Carry the server's `PhysicsState` word to the *parts* —
    /// applying the `NODRAW_PS` and `HIDDEN_PS` reactions to the part array.
    ///
    /// Always on in the client: the three state-bit tests run unconditionally, including on
    /// every create. Clearing it still delivers the state word to the *body*, but
    /// `StateSideEffects` is discarded before it reaches a part — which is the control the
    /// differential needs.
    ///
    /// **This does not hide a `HIDDEN_PS` object**, and that is not an omission. Nothing in
    /// the client's draw path reads `HIDDEN_PS`: it walks the cell's shadow-part list
    /// with no state test, and setting hidden never touches
    /// its own part array. What `HIDDEN_PS` does is force `NODRAW_PS` onto the object's
    /// **children**, and that is what this switch turns on. The hidden-and-no-draw tests cover
    /// both.
    pub object_state_draw: bool,
    /// Skip a part whose view-cone test answers OUTSIDE when there is no
    /// active portal list. Only a non-OUTSIDE answer reaches mesh drawing on that branch.
    ///
    /// **It gates the DRAW ONLY. It does not gate the cone and it does not gate the latch.**
    /// The cone runs on every submitted part on both arms, the drawn world's `drawn_object_cone`
    /// reports it on both, and the inside-view-cone latch reads the same real answer on both. See
    /// the drawn world's `part_cone` and the loop in the drawn world's `draw`, where the `continue`
    /// this flag controls sits *before* `draw_part` and the latch test reads `status`
    /// regardless of it. That is asserted rather than asserted-by-comment, in
    /// `the_shipping_default_is_the_clients_cull_and_the_latch_is_real`.
    ///
    /// **`true` is the shipped default and it is the client's behaviour**, and a pixel
    /// differential backs it. The measurement is the view-cone pixel test: one driven scene —
    /// the capture corpus replayed to its
    /// most populated datagram, over its own landblock at the shipped land and scenery radii —
    /// with the same pose and the same frame number on both arms and this flag the only
    /// difference, over eleven camera stations. **0 of 307 200 pixels differ at every one**,
    /// against a rebuild-versus-rebuild control that is itself exactly **0**. That the cull ran
    /// is asserted rather than assumed at each station: `culled == 0` on the off arm and between
    /// 177 and 579 of 579 tested parts culled on the on arm, with the subsets reaching the
    /// device falling from 873 to as few as 0.
    ///
    /// Off is therefore not a deviation that is merely *counted* — it is a build that
    /// draws meshes the client would reject. The flag stays because
    /// three benches park a camera without aiming it and need the pre-cull
    /// submission to measure their sort key, and because `ObjectConeStats::outside` versus
    /// `ObjectConeStats::culled` is still the pair that tells *"the cull did not run"* from
    /// *"the cull rejected nothing"*.
    pub object_viewcone: bool,

    /// Run the outdoor pass for an **indoor** viewer only when portal traversal
    /// reached the outdoors.
    ///
    /// Always on in the client. An outdoor viewer reaches the outdoor pass directly;
    /// an indoor viewer reaches it through the outside-view list, populated once per
    /// **visible portal whose
    /// `other_cell_id == 0xFFFFFFFF`**. A dungeon with no outdoor portal in view draws no
    /// terrain, no scenery and **no sky**.
    ///
    /// Clearing it runs the whole outdoor pass, every frame, indoors as well, which is the
    /// control the differential needs. That puts a flat light backdrop behind every gap in a
    /// dungeon's cells: the sky is drawn
    /// first, at `DEPTHTEST_ALWAYS` with the depth write off, so anything the interior does
    /// not cover keeps it.
    pub outside_view_gate: bool,

    /// An indoor viewer whose cell has **not** `seen_outside`
    /// reaches neither landblock-window update,
    /// so the landblock window does not move. See the drawn world's `viewpoint_block`.
    ///
    /// **This is the switch between the two *derivations* of the window's block.** Set, the
    /// window's block is the landblock of the **cell id** supplied by the normal rendering
    /// path; clear, it is `floor(origin / 192)` applied to the viewpoint. A third arm bolted
    /// onto the origin arithmetic rather than replacing it lets a teleport into a dungeon slip
    /// between the two — see the drawn world's `viewpoint_block` and the indoor-draw tests.
    ///
    /// Always on in the client, which has no switch. Clearing it re-centres the window from
    /// the viewpoint's **origin** on every frame, which is the control the differential needs.
    /// That walks the window off a dungeon's own
    /// landblock, because a dungeon's cell frames are its own layout space and 87 % of the
    /// retail dat's interior cells sit outside their block's `[0, 192]²`; the block then
    /// stops being resident and the drawn world's `draw_inside` draws nothing at all.
    pub indoor_viewpoint_gate: bool,

    /// Clear the depth buffer and apply the portal
    /// depth stamps that follow it, the two steps that make an indoor frame's *interior* draw
    /// over the outdoor pass instead of being z-buffered against it.
    ///
    /// ```text
    /// if the outside-view list is nonempty {
    ///     draw the landscape through that portal list;
    ///     flush translucent geometry and advance the frame stamp;
    ///     if depth-clear is requested or any portals were drawn, clear depth;
    ///     stamp each portal polygon at its own depth;
    /// }
    /// draw the interior cells;
    /// ```
    ///
    /// Clear mask 4 is **depth only** (in the engine's flag word bit 4 is the depth
    /// buffer), so the landscape's *colour* stays and its *depth* does not. The interior then
    /// draws over it unconditionally, except inside the openings stamped back at the
    /// polygon's own depth with mask **6** (bit 1 alpha 0, bit 2 depth
    /// write, bit 0 clear so the depth is the polygon's own rather than the far constant the
    /// building half's mask 7 uses). So the land does not draw over the room, and the land
    /// seen *through the window* still draws.
    ///
    /// Always on in the client, which has no switch. Clearing it leaves the outdoor pass's
    /// depth standing in front of everything the interior draws below it, which is the
    /// control the differential needs.
    ///
    /// The stamp half is additionally gated by [`Self::portal_depth_stamp`], because it is the
    /// same portal-polygon draw operation used by the outdoor building pass, and that flag already
    /// names it. Clearing *that* alone gives the third arm: the Z clear with no openings
    /// stamped back, which is what a blanket "do not draw the land indoors" would look like.
    pub indoor_z_clear: bool,
    /// Require `side_cell_count == 8` for a block's
    /// **static objects and buildings**, not only on its scenery.
    ///
    /// Static-object and building initialization both return unless this count is eight.
    /// A change away from full detail destroys all three populations. So a landblock on an
    /// outer LOD ring is bare terrain in the client.
    ///
    /// Always on in the client, which has no switch. Clearing it leaves the guard on
    /// `generate_scenery` alone, and that is the control the LOD object-guard differential is
    /// taken against.
    ///
    /// **It changes counts and a bake, and no pixel.** It is reachable only at
    /// `scenery_radius >= 2` (rings 0 and 1 are both full detail and the shipped
    /// `scenery_radius` is 1), and even there the objects it stops baking were assembled and
    /// never submitted: a coarse ring begins 288 m out and the degrade record's terminator has
    /// already taken them. The test measures that rather than assuming it.
    pub lod_object_guard: bool,
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            landblock: DEFAULT_LANDBLOCK,
            start_cell: None,
            cell_statics: true,
            mesh_collision: true,
            part_degrades: true,
            // Quality preset 1 (the presets select radii 3, 5, 8, 11,
            // 15). The landscape constructor default is 5 = 121 blocks, which is 2.5x the texture
            // merges and 2.5x the draws for a slice that only has to be looked at.
            //
            // **This is the bench default, not the client's.** The client
            // takes its window from `Render.LandscapeDrawDistance`, whose registered default
            // is **8** — see [`crate::config::Config::land_radius`]. The 3 here is kept
            // because a test wants the cheapest window that still has rings in it, and every
            // suite in the tree that names a radius names its own; a test that wants the
            // client's answer asks `App::config()` for it, as a dozen of them do.
            land_radius: 3,
            scenery_radius: 1,
            stream_budget: None,
            gpu_terrain_merge: true,
            terrain_splat: false,
            render: crate::render_prefs::RenderPreferences::default(),
            camera_height: 45.0,
            character: true,
            time_of_day: None,
            game_time: None,
            // World fog is enabled by default.
            world_fog: true,
            camera: crate::camera::CameraPreferences::default(),
            mouse_look: crate::actions::camera::MouseLookPreferences::default(),
            particles: true,
            building_portals: true,
            portal_clip: true,
            portal_depth_stamp: true,
            portal_alpha_flush: true,
            auto_degrades: false,
            surface_translucency: true,
            material_translucency: true,
            object_lighting: true,
            degrade_levels: true,
            part_degrade_levels: true,
            static_billboards: true,
            part_billboards: true,
            part_alpha_lists: true,
            part_depth_sort: true,
            release_interiors: true,
            object_state_draw: true,
            // **`true` matches the client's behavior**, backed by a pixel differential (0 of
            // 307 200 pixels over eleven stations, control 0, 177-579 of 579 parts culled per
            // station). See the field's own documentation.
            object_viewcone: true,
            outside_view_gate: true,
            indoor_viewpoint_gate: true,
            indoor_z_clear: true,
            lod_object_guard: true,
        }
    }
}
