//! The landscape residency window and normal-mode viewpoint — which landblocks are
//! resident, which block the world is drawn around, and which cell of it the viewer stands in.
//!
//! The scene owns a `WorldStreamer` and drives it; the streamer owns the `LandblockWindow` and
//! the viewer cell index, and answers the scene's viewpoint questions. It makes no device call:
//! `LandblockWindow::update_block` hands back a list of `SlotAction`s and the **scene** is what
//! turns those into bakes and uploads.

use std::any::Any;

pub use dereth_landscape::SlotAction;
use dereth_landscape::BLOCK_LENGTH;
use dereth_primitives::{CellId, Vec3};

use crate::camera::FreeCamera;
use crate::character::{Character, RenderSpace};
use dereth_world_data::landblock::block_xy;

/// A window slot's geometry, as the drawing side built it.
///
/// The window keeps it resident and scrolls it with its block, and releases it when the block
/// leaves; what is inside is the drawing side's own mesh type, which only the drawing side reads
/// back out ([`SlotMesh::get`]).
pub struct SlotMesh(Box<dyn Any + Send + Sync>);

impl SlotMesh {
    /// Hold `mesh` in a window slot.
    #[must_use]
    pub fn new<T: Any + Send + Sync>(mesh: T) -> Self {
        Self(Box::new(mesh))
    }

    /// The mesh, if it is a `T`.
    #[must_use]
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl std::fmt::Debug for SlotMesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SlotMesh(..)")
    }
}

/// The landscape window, with each slot holding the drawing side's mesh.
pub type LandblockWindow = dereth_landscape::LandblockWindow<SlotMesh>;

/// The residency window over the landscape, plus the viewpoint reads that decide where it is
/// centred.
#[derive(Debug)]
pub struct WorldStreamer {
    /// The resident landblocks and their slots — the sliding window.
    pub window: LandblockWindow,
    /// Draw-order square coordinates: which cell of the viewer's own block the viewpoint
    /// is in, `0..=7` on each axis.
    pub viewer_cell: (u8, u8),
    /// The landblock the viewpoint's cell is in, which the drawing is ordered around. It is the
    /// window's own block unless the camera stands across a block line from the player.
    pub viewer_cell_block: Option<(i32, i32)>,
}

impl WorldStreamer {
    /// A window of `land_radius` blocks either side, with no viewpoint yet.
    #[must_use]
    pub fn new(land_radius: u32) -> Self {
        Self {
            window: LandblockWindow::new(land_radius),
            viewer_cell: (0, 0),
            viewer_cell_block: None,
        }
    }

    /// The block the window is centred on, for the tests and the log lines.
    #[must_use]
    pub fn viewer_block(&self) -> Option<(i32, i32)> {
        self.window.viewer_block()
    }

    /// `SceneConfig::scenery_radius`: which window slots grow scenery, buildings and statics.
    ///
    /// The client has no such knob — dynamic-object initialization runs for every resident block
    /// — so this is a rebuild-only budget, and it is the only reason a block's objects are not
    /// simply a function of "is it in the window".
    pub fn wants_objects(&self, xi: u32, yi: u32, land_radius: u32, scenery_radius: u32) -> bool {
        #[allow(clippy::cast_possible_wrap)]
        // LINT-OK: window radii, at most 15. Not a float conversion.
        let (r, sr) = (land_radius as i32, scenery_radius as i32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        // LINT-OK: window indices, bounded by mid_width <= 31.
        let (dx, dy) = ((xi as i32) - r, (yi as i32) - r);
        dx.abs() <= sr && dy.abs() <= sr
    }

    /// Update viewpoint residency: re-centre the window when the
    /// player crosses a landblock boundary.
    ///
    /// The window follows the **player**, not the camera: see [`Self::load_point_block`]. The
    /// camera only orders the drawing ([`Self::update_viewer_cell`]), so a turn on the spot
    /// that swings the camera across a block line moves nothing.
    ///
    /// The whole landscape is drawn in a space whose origin is the south-west corner of the
    /// viewer's block, so re-centring moves that origin: everything expressed in it — the free
    /// camera here, and (through their own accessors) the body and the server's objects — is
    /// shifted by the same whole number of blocks. Nothing is re-transformed, because nothing is
    /// *baked* in that space.
    ///
    /// Returns the block shift, for the tests, and **the frame's [`RenderSpace`] token**.
    ///
    /// # The token
    ///
    /// Choosing the viewer block is the event every writer of a drawn frame has to be after
    /// (the block origin is folded into the part frames, so a writer that runs first draws in
    /// the wrong block's space). `Character::set_viewer_block` mints the proof that it happened,
    /// and a writer takes one by value — so a call placed **above** this one cannot name the
    /// binding and does not compile. See [`RenderSpace`], which also says what the token does *not*
    /// cover.
    ///
    /// `set_viewer_block` is therefore called on **every** frame with a body, not only on one
    /// that crosses a boundary. The assignment is idempotent, and the non-crossing frame needs
    /// a token too; this is the same reasoning
    /// `place_local_body` gives for being unconditional itself.
    ///
    /// A token comes back on **every** path, including the two that have no `Character` to
    /// mint from — a bodiless scene, and the moment before the window has a block, where
    /// `render_frame_of` falls back to `cfg.landblock`. Both of those still choose the
    /// block the frame is drawn in, and `advance_objects` still has objects to place
    /// in it, so making the token optional there would have been a behaviour change wearing a
    /// type's clothes.
    pub fn recenter(
        &mut self,
        character: &mut Option<Character>,
        camera: &mut FreeCamera,
        free_cell: Option<CellId>,
        indoor_viewpoint_gate: bool,
        fallback_landblock: u16,
    ) -> ((i32, i32), RenderSpace, Option<Vec<SlotAction>>) {
        let Some(cur) = self.window.viewer_block() else {
            let fallback = block_xy(fallback_landblock);
            // No window yet, so no `update_block` and **no bake queue call at all** -- which
            // is not the same as calling it with an empty list: `WorldScene::queue` derives its
            // departed set from the actions it is given, so an empty list would release every
            // resident block. `None` is "the scene must not call it".
            return (
                (0, 0),
                RenderSpace::for_a_window_without_a_body(fallback),
                None,
            );
        };
        // The block is `load_point_block` — the landblock of the **player's cell id**, which is
        // the only thing the client scrolls its window by. The landblock is the cell id's top
        // 16 bits.
        //
        // The `false` arm is the **origin-derived fallback**, kept for the differential
        // [`SceneConfig::indoor_viewpoint_gate`] documents: re-derive the block from the
        // viewpoint's *origin*, `floor(origin / 192)`, which is not landblock coordinates
        // inside a dungeon — 642,001 of the retail dat's 734,976 interior cells sit outside
        // their block's `[0, 192]²`, and the shift then walks the window off the very
        // landblock whose cells are being drawn.
        // **The body-less arm.** `load_point_block` opens
        // `let c = character.as_ref()?`, so on the `--no-character` flycam it answers
        // `None` for a reason that has nothing to do with retail's zero cell id — there is
        // no player and no cell id *at all*, not a cell id the window cannot
        // use. Reading that `None` as `load_point_block().unwrap_or(cur)` turns "the flycam has
        // no cell id" into "keep the block you have", which is a window that **never moves
        // however far the flycam flies**.
        //
        // The two `None`s must therefore be told apart, and the flycam's answer is
        // `floor(origin / 192)` from `viewpoint`, which for a
        // body-less viewer *is* the flycam's own position and is in landblock space because the
        // flycam only ever flies outdoors. The cell-id arm still covers the 87 % of
        // interior cells authored outside their block's `[0, 192]²`: they are
        // reachable only through a `Position`, i.e. only with a body.
        //
        // A free camera whose front end names the room it is in
        // (`WorldState::free_camera_cell`) is held on that room's landblock, as a body in it
        // would be: the room may well lie outside its block's square.
        let free_block = free_cell
            .filter(|c| !dereth_physics::landdefs::is_outdoors(*c))
            .filter(|_| character.is_none())
            .map(|c| {
                #[allow(clippy::cast_possible_wrap)] // a landblock index, 0..=255
                (((c.0 >> 24) & 0xFF) as i32, ((c.0 >> 16) & 0xFF) as i32)
            });
        let next = if let Some(block) = free_block {
            block
        } else if indoor_viewpoint_gate && character.is_some() {
            self.load_point_block(character).unwrap_or(cur)
        } else {
            let p = viewpoint(character, camera);
            (
                cur.0 + dereth_primitives::num::floor_to_i32(p.x / BLOCK_LENGTH),
                cur.1 + dereth_primitives::num::floor_to_i32(p.y / BLOCK_LENGTH),
            )
        };
        let (dx, dy) = (next.0 - cur.0, next.1 - cur.1);
        if dx == 0 && dy == 0 {
            // The player did not cross a boundary: no `update_block`, and so no bake queue
            // call. See the note above on why this is `None` and not an empty list.
            return ((0, 0), choose_viewer_block(character, cur), None);
        }
        #[allow(clippy::cast_precision_loss)] // a block shift, at most 255
        let (sx, sy) = (dx as f32 * BLOCK_LENGTH, dy as f32 * BLOCK_LENGTH);
        camera.position.x -= sx;
        camera.position.y -= sy;
        let space = choose_viewer_block(character, next);
        // `LandblockWindow::update_block` decides **what** must be baked and released; turning that
        // list into bakes and GPU releases is the scene's, so the actions go back to it.
        let actions = self.window.update_block(next);
        ((dx, dy), space, Some(actions))
    }

    /// The load point's block: where the landscape window is centred, read from the **player's**
    /// position.
    ///
    /// The client keeps two positions for its landscape. Once a frame the world update moves
    /// the landscape's load point to the player's position, and only that scrolls the window:
    /// it decides which blocks are resident, each block's ring and level of detail, and so
    /// which blocks carry their objects. The camera's cell reaches the landscape only through
    /// the drawing, which orders the blocks and cells around the camera's block and moves
    /// nothing ([`Self::update_viewer_cell`]). A turn on the spot swings the camera, and
    /// Horizon's camera stands six metres or more from the player, across a block line
    /// without moving the player, and scrolls nothing.
    ///
    /// ```text
    /// if the player's cell is outdoors:
    ///     the load point is the player's cell
    /// else if the player's cell can see outdoors:
    ///     the load point is the player's outside-cell mapping
    /// else:
    ///     the landscape is released
    /// ```
    ///
    /// The third arm is not an optimisation: a
    /// dungeon cell's position is in the **dungeon's own layout space**, not landblock
    /// coordinates. Over the retail cell dat, **642,001 of 734,976 interior cells (87 %, in
    /// 1,819 landblocks) are placed outside their block's `[0, 192]²`** — the training academy
    /// runs from `y = -250` to `y = 0` — so re-deriving a block from the origin walks the
    /// window off the landblock whose cells are being drawn, and `draw_inside` then finds its
    /// own start cell absent and draws nothing. The gate is exactly `seen_outside`, and the
    /// data agrees the two populations barely overlap: of those 642,001 cells, **222** are also
    /// `seen_outside`, all within 13 m of the box (a building straddling a block edge, not a
    /// dungeon layout).
    ///
    /// # The third arm is a *block*, not a refusal
    ///
    /// Transcribing the branch as "on the third arm, do not move the window" while leaving
    /// `recenter`'s `floor(origin / 192)` standing on the other two arms breaks on arrival:
    /// **teleporting** into Drudge Hideout
    /// (`0x019E0114`, authored at `y = -40`) from another landblock, the destination cell is
    /// not yet resident, so the refusal above does not fire, the origin arithmetic re-centres
    /// the window **one block south** of the dungeon — `(1, 157)` for `(1, 158)` — and from
    /// there the cell can never become resident, so the refusal never fires again and the
    /// window never corrects itself. `traversal_cells` then holds no cell of `0x019E`,
    /// `draw_inside`'s `cells.contains_key(&start.0)` fails, and the dungeon draws nothing at
    /// all while its doors and its drudges, which `draw_in_viewport` places from
    /// `advance_objects` regardless, keep drawing. Logging in *inside* the dungeon was fine
    /// because `SceneConfig::landblock` names the block before the first frame and there is
    /// nothing to re-centre.
    ///
    /// So the arms answer a **landblock** here, out of the cell id, and never out of an origin.
    ///
    /// **The third arm's deviation, stated.** The client releases its landscape there, and this
    /// returns the player cell's own landblock instead. The two are the same thing everywhere
    /// the client can reach: the landscape window is the *outdoor* draw list; interior cells
    /// come from the cell manager, and a player who has been in that dungeon since it loaded
    /// already has the window where its cells are. This build cannot release the window
    /// there, because the same window is where its interior cells come from. Answering "the
    /// cell's own block" is the one answer that recovers; "keep what you had" is the one that
    /// cannot.
    ///
    /// **A scene with no body** (the flycam) has no player to read and
    /// answers `None`. **That `None` is not retail's zero cell id and `recenter`
    /// must not read it as "keep the block you have"**, which would freeze the
    /// `--no-character` window in its starting block for ever. `recenter` takes the body-less
    /// arm before it reaches this function; see the comment on its own `next` binding.
    pub fn load_point_block(&self, character: &Option<Character>) -> Option<(i32, i32)> {
        use dereth_physics::landdefs as ld;
        let c = character.as_ref()?;
        let pos = c.position();
        let cell = pos.cell;
        let id = if ld::is_outdoors(cell) {
            cell
        } else if cell_seen_outside(character, cell) {
            ld::get_outside_cell_id(cell, pos.frame.origin)
        } else {
            // The third arm, as a block. See the note above.
            cell
        };
        #[allow(clippy::cast_possible_wrap)] // a landblock index, 0..=255
        (id.0 != 0).then_some((((id.0 >> 24) & 0xFF) as i32, ((id.0 >> 16) & 0xFF) as i32))
    }

    /// Update the viewpoint's cell coordinates: which cell of its own block the
    /// viewer stands in, which is what the per-block cell draw ordering orders each block's cells
    /// around, and which landblock that cell is in, which the blocks are ordered around.
    ///
    /// This is the camera's, where the window's block is the player's
    /// ([`Self::load_point_block`]): the camera orders the drawing and moves nothing. It runs
    /// *after* `recenter`, the same frame's window.
    ///
    /// # It is a reading of the viewpoint's cell **id**, not of its position.
    ///
    /// Normal-mode rendering supplies one 32-bit cell id. The viewpoint's draw-order update
    /// converts that id to land coordinates and masks **both** coordinates with 7 before
    /// passing them to the landblock's draw-order calculation.
    ///
    /// So the index is **masked, never clamped**, and it cannot leave `0..=7` in the first
    /// place: coordinate conversion builds `x` as `blockX*8 + ((idx - 1) >> 3)` from the
    /// *same* id whose top half named the block, so the two halves are one number read twice
    /// and cannot disagree. Deriving them **separately** — the block from `floor(p / 192)` and
    /// the cell from `floor(p / 24)` — and clamping the second to `0..=7` is not the same
    /// thing. A clamp is not the client's `& 7`: they agree
    /// only while the viewpoint is inside the block the window is centred on, and where they
    /// part the clamp reports a cell the viewer is **not** in.
    ///
    /// A clamp also hides its own defect. At a *block* boundary both derivations saturate to
    /// the same edge cell, so a test that crosses landblocks sees nothing; the disagreement is
    /// only visible at the 24 m boundaries **inside** a block. The viewer-cell tests assert at
    /// both.
    ///
    /// **The two arms are the world render pass's own.** Outdoors it passes
    /// the viewer's `objcell_id` through untouched — so this does too, rather than
    /// re-deriving an id from the origin — and indoors it passes
    /// the viewer's outside-cell mapping, the land cell the building
    /// stands in, because `gid_to_lcoord` rejects an interior id outright (`(id & 0xFFFF) <
    /// 0x100`). That indoor conversion is why the arms matter: it is what the per-block cell
    /// draw ordering receives inside a dungeon.
    ///
    /// **A viewpoint with no usable id leaves the previous value standing**, which is what the
    /// client does: `calc_draw_order` returns at once on a zero id, so a zero id touches neither
    /// the draw order nor anything else.
    pub fn update_viewer_cell(&mut self, character: &Option<Character>, camera: &FreeCamera) {
        if let Some(xy) = self.viewpoint_cell(character, camera) {
            self.viewer_cell = xy;
            #[allow(clippy::cast_possible_wrap)] // a landblock index, 0..=255
            let block = |id: CellId| (((id.0 >> 24) & 0xFF) as i32, ((id.0 >> 16) & 0xFF) as i32);
            self.viewer_cell_block = self.viewpoint_cell_id(character, camera).map(block);
        }
    }

    /// Where the viewpoint's block sits in the window, as a window index `(xi, yi)`: the block
    /// the drawing is ordered around. The window's own centre when the viewpoint's block is not
    /// known or not in the window.
    #[must_use]
    pub fn viewer_window_index(&self) -> (u32, u32) {
        let r = self.window.mid_radius();
        let centre = (r, r);
        let (Some((cx, cy)), Some((vx, vy))) = (self.window.viewer_block(), self.viewer_cell_block)
        else {
            return centre;
        };
        #[allow(clippy::cast_possible_wrap)] // the window radius, at most 25
        let ri = r as i32;
        let (xi, yi) = (vx - cx + ri, vy - cy + ri);
        let w = 2 * ri + 1;
        if (0..w).contains(&xi) && (0..w).contains(&yi) {
            #[allow(clippy::cast_sign_loss)] // both checked non-negative on the line above
            (xi as u32, yi as u32)
        } else {
            centre
        }
    }

    /// The cell id normal-mode rendering hands to the landscape viewpoint update.
    ///
    /// `None` is the client's zero cell id: no viewpoint this frame. See
    /// `update_viewer_cell` for the two arms and why they are the client's.
    ///
    /// # The id is not a reconstruction of the origin, and on a cell line it disagrees
    ///
    /// The outdoor arm passes `viewer.objcell_id` **through**, and that is not the same thing
    /// as re-deriving it from `viewer.frame.origin`. Viewer updating
    /// takes the transition's `sphere_path.curr_pos`, whose cell is whatever cell-list
    /// discovery last answered — and **that is not
    /// `floor(origin / 24)`**. On a 24 m line the point sits on a cell edge, and
    /// the 2D point-in-polygon test rejects only on `v > 0`, so it is inside the
    /// cells on *both* sides; the container loop assigns on every match and breaks only for an
    /// interior cell, so the **last** land cell of the array wins, and
    /// cell-boundary collection puts the `-1` neighbour into the array
    /// after the `floor` cell. The id is therefore the cell *below* the line, and it is the
    /// one the client draws with.
    ///
    /// The id is **not** "the cell the swept sphere travelled into — a fact about the path":
    /// nothing
    /// in cell-list discovery, outdoor-cell collection or cell-boundary collection
    /// reads an earlier position, and the transition's other-cell check recomputes
    /// it at the end of every sub-step. It is a pure function of the position, the block its
    /// origin is expressed in and the sphere's radius. Measured over 49 resting positions
    /// across a cell line, each approached from both sides: **0** whose answer depends on the
    /// direction.
    ///
    /// Measured on two 600-frame runs north: at `x = 96.0`,
    /// exactly the `4 * 24` line, they name different cells on **600 of 600** frames; four
    /// metres inside a cell, on **0 of 600**. Both walks are reported because exact axis
    /// alignment is the trap in both directions — the aligned
    /// fixture makes this fire always and the unaligned one makes it vanish, and neither
    /// alone is the honest number.
    pub fn viewpoint_cell_id(
        &self,
        character: &Option<Character>,
        camera: &FreeCamera,
    ) -> Option<CellId> {
        use dereth_physics::landdefs as ld;
        let id = match character.as_ref().filter(|c| c.camera.attached()) {
            Some(c) => {
                let pos = c.camera.viewer;
                if ld::is_outdoors(pos.cell) {
                    pos.cell
                } else {
                    ld::get_outside_cell_id(pos.cell, pos.frame.origin)
                }
            }
            // No body at all: there is no body-derived viewer, and the flycam *is* the
            // viewpoint (the `--no-character` path). The client has no such state, so there is
            // no arm to copy — what is copied is the outdoor-coordinate adjustment,
            // applied to the window's own block and the flycam's block-relative origin, which
            // is the same arithmetic `get_outside_cell_id` runs on a real viewer.
            None => {
                let (bx, by) = self.window.viewer_block()?;
                ld::get_outside_cell_id(ld::lcoord_to_gid(bx * 8, by * 8), camera.position)
            }
        };
        // `calc_draw_order`'s own zero-id early return,
        // written out because it is the client's shape rather than because it changes an
        // answer here: `gid_to_lcoord` rejects `CellId(0)` too, through
        // `inbound_valid_cellid`, so `viewpoint_cell` answers `None` either
        // way.
        //
        // **The two checks are a redundant pair, and each masks a mutation of the other.**
        // Deleting this line survives (`gid_to_lcoord` still refuses), and turning
        // `viewpoint_cell`'s second `?` into `unwrap_or((0, 0))` also survives (this line
        // returns `None` before that `?` is reached). The code and the tests are right: the two
        // guards protect the same state, so no single-point mutation is visible. The behaviour
        // itself is asserted at the zero-id station of the viewer-cell tests, by a mutation at
        // the one site downstream of both
        // (`update_viewer_cell`'s `unwrap_or((0, 0))`), which reddens.
        (id.0 != 0).then_some(id)
    }

    /// Mask both coordinates of `viewpoint_cell_id` with 7 to obtain the square coordinates
    /// passed to landblock draw ordering and then to the per-block cell draw ordering as the closest cell.
    pub fn viewpoint_cell(
        &self,
        character: &Option<Character>,
        camera: &FreeCamera,
    ) -> Option<(u8, u8)> {
        let (x, y) =
            dereth_physics::landdefs::gid_to_lcoord(self.viewpoint_cell_id(character, camera)?)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: `& 7` bounds both to 0..=7. Not a float conversion.
        Some(((x & 7) as u8, (y & 7) as u8))
    }
}

/// The normal-mode renderer's `seen_outside` flag for the viewer cell.
///
/// A cell the land source does not hold answers **`false`**, which is the third arm. The
/// client dereferences a live cell and cannot reach the not-loaded state, but answering `true`
/// here would run `get_outside_cell_id` on a dungeon origin and walk the window off the
/// dungeon. `false` is the
/// arm that asks for the cell's own block, which is the block that makes the cell resident
/// and so the only answer that can correct itself on the next frame.
pub fn cell_seen_outside(character: &Option<Character>, cell: CellId) -> bool {
    character.as_ref().is_some_and(|c| {
        dereth_physics::LandSource::env_cell(c.land().as_ref(), cell)
            .is_some_and(|g| g.seen_outside)
    })
}

/// Re-anchor the body on `block` and mint the frame's [`RenderSpace`].
///
/// The body's arm is `Character::set_viewer_block`, which is the real act; the bodiless
/// arm names the same block through the crate-private producer. Both are here rather than
/// inline in `recenter` so that "choosing the viewer block" is one named thing
/// with one return value.
pub fn choose_viewer_block(character: &mut Option<Character>, block: (i32, i32)) -> RenderSpace {
    character.as_mut().map_or_else(
        || RenderSpace::for_a_window_without_a_body(block),
        |c| c.set_viewer_block(block),
    )
}

/// The viewpoint is handed to the landscape in the renderer's viewer-block-relative space.
///
/// **The render viewer is the swept camera, never the debug chase
/// camera.** Read the call site: it updates the landscape with the viewer's cell, using the
/// viewer's outside-cell mapping on the seen-outside arm and the viewer on the next line either
/// way. **One value decides the block, the cell and the eye**, and the renderer splits it itself:
/// it re-runs `calc_draw_order` whenever
/// the new cell id differs from `viewer_cell_id`, with a "the block moved" flag that is exactly
/// `((old ^ new) & 0xFFFF0000) != 0`. So the block half and the cell half are two readings
/// of one number, and taking them from two different cameras is not a thing the client
/// can do.
///
/// Here it could: `follow_character` runs inside `update`
/// and **overwrites the flycam** with the debug chase camera before `recenter`
/// reads it, so reading the flycam takes both halves from a viewpoint the shipped client never
/// draws with: `dereth_client_runtime::camera::update_viewer` replaces the flycam outright a few
/// lines later in `App::frame`. The two cameras are not the same place — the chase camera
/// sits `camera_distance` (4.5 m) behind the body along its heading and passes
/// through walls, where the swept viewer sits at the camera's configured offset behind
/// the *pivot* and is pulled in by the swept sphere — so the answers differ whenever a
/// boundary falls between them. That is every crossing for the 192 m block half and a
/// large fraction of all walking frames for the 24 m cell half.
///
/// **There is no circularity in reading it here**, which is what makes this wiring
/// possible at all even though the sweep runs after `WorldScene::update`:
/// `CameraControl::viewer` is a [`dereth_primitives::Position`] — a cell id and a *cell-local*
/// frame — so it does not live in the space the re-centre moves, and
/// [`Character::render_frame_of`] re-expresses it in whatever viewer block is current at
/// the moment of the call. What the frame order does cost is one frame of *lag*: the
/// client sweeps and re-centres inside the same draw-without-blitting call
/// (`update_viewer` then the world render pass), where `App::frame` re-centres from the
/// sweep the previous frame left. That is a lag in the camera's motion, not an error in
/// its space, and it is bounded by one frame of camera travel where reading the chase camera
/// would be a standing 4.5 m offset in the wrong direction.
///
/// Without a body there is no body-derived viewer and the flycam *is* the
/// viewpoint, which is the `--no-character` path.
///
/// # A harness that drives `update` without the sweep has a frozen viewpoint
///
/// **`WorldScene::update` on its own is not a frame**, and this is where that becomes
/// visible. `Character::camera.viewer` is written by the camera update and by resetting the viewer
/// from a position (which is what [`Character::teleport`] calls),
/// and by nothing else — so a loop that calls `update` and never calls
/// `dereth_client_runtime::camera::update_viewer` leaves the viewpoint wherever the last teleport put
/// it, and the window will not re-centre however far the body walks. A harness that walks
/// the body must run `App::frame`'s order — `update` then `camera::update_viewer` — which
/// moves the viewpoint with the body. A harness that steps the body by teleporting
/// re-attaches the camera, so it does not see the problem.
///
/// The client cannot reach that state, because its sweep is unconditional and sits on the
/// line before the viewpoint is read. It is a **number** rather than a comment:
/// `dereth_scene::world_scene::SceneStats::updates_without_a_sweep` counts exactly this, on the update it happens,
/// instead of leaving it to redden whichever test twenty frames downstream happened to
/// depend on the window moving.
///
/// # The lag is measured, and the order is **pinned** rather than moved
///
/// The sweep-lag test walks five stations in `App::frame`'s own order and reads
/// the per-frame viewpoint out of `camera.viewer` in global metres and a global cell id:
///
/// | station | body | one frame of camera travel | as a fraction of a 24 m cell |
/// |---|---|---|---|
/// | run north, exactly on the `x = 24` line | 3.972 m/s | max **0.152 m** | 0.63 % |
/// | run north, 4 m inside the cell | 3.972 m/s | max **0.152 m** | 0.63 % |
/// | walk north, 4 m inside | 2.571 m/s | max **0.099 m** | 0.41 % |
/// | run on a 37-degree heading | 3.972 m/s | max **0.192 m** | 0.80 % |
/// | run 400 m, across the block boundary | 3.993 m/s | max **0.971 m** | 4.05 % |
///
/// The aligned and offset stations agree to six decimals, so the answer is not an
/// artefact of the axis alignment. The 0.971 m outlier is
/// a **camera released from a wall**, not a following camera: excluding frames adjacent to
/// a `sweeps_blocked` the same run's worst step is far smaller, and the file reports both.
///
/// So over 5,400 frames, 33 take a different cell decision and 3 a different block
/// decision, and the largest change in cell index across any single frame is **1** — the
/// split order reaches each boundary one frame (1/30 s) late and **never skips one**. That
/// is below one cell boundary at every station, which is the condition for pinning the order,
/// so it is pinned.
///
/// # Why the sweep's placement is still right, which is the other half of the answer
///
/// The reason, quoted from the sweep's call site in `crate::app::App::frame`: *"It runs
/// here rather than inside `world.update` because the sweep starts from the pivot the body
/// has just moved to and must be expressed in the block the re-centre has just chosen."*
///
/// The first clause holds and does not pin the placement — the sweep would still start
/// from the moved pivot at a site inside this function after `Character::update`. **The
/// second clause holds and does pin it**, for a half of the step it is easy not to notice.
/// `CameraControl::update_viewer` — the sweep itself — is block-**independent**: it works
/// in `Position` (a cell id and a cell-local frame) and in physics space and never
/// consults the viewer block, which is exactly why reading `viewer` here is not circular.
/// But `dereth_client_runtime::camera::update_viewer` also does `scene.camera =
/// FreeCamera::from_frame(&c.camera_render_frame()?)`, and `camera_render_frame` goes
/// through [`Character::render_frame_of`], which subtracts the **current**
/// `viewer_block`. Move that call above `recenter` and the render camera is left
/// expressed in the block the frame is *leaving* — a 192 m error once per crossing, in
/// exchange for a 0.97 m worst-case lag.
///
/// A faithful move is therefore not a move but a **split**: the block-independent sweep
/// above the re-centre and the block-dependent projection below it, with the projection
/// becoming a third writer of the drawn frame and wanting a [`RenderSpace`] of its own.
/// That is a real change to the frame order, and the measurement above does not pay for
/// it. If the lag ever does matter — a faster camera, a variable step, a smoother that
/// overshoots — the sweep-lag test is what will say so, and the split is the
/// shape the change should take.
pub fn viewpoint(character: &Option<Character>, camera: &FreeCamera) -> Vec3 {
    character
        .as_ref()
        .and_then(Character::viewer_render_frame)
        .map_or(camera.position, |f| f.origin)
}
