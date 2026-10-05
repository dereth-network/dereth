//! World-object picking and the geometry it sweeps.
//!
//! **This file wires; it does not implement.** The algorithm is
//! [`crate::pick_geometry`]; the tables are
//! [`crate::objects::ObjectStream`]'s; the decode is [`dereth_assets`]'s. What is this file's own
//! is the selection controller's one-frame object-search request, the candidate list the
//! renderer would have produced, and the read-out after drawing without blitting.
//!
//! # The shape of the client's pick, preserved
//!
//! ```text
//! UI mouse event
//!  └─ find the object at `(x, y)`           -> arm a one-frame selection request
//! Client simulation
//!  ├─ draw the scene without blitting
//!  │    ├─ update viewpoint -> selection_ray = pick_ray(the stored selection point)
//!  │    ├─ test each drawn mesh against the selection ray
//!  │    ├─ if an object was requested: read the hit and report the found object
//!  │    └─ clear the render selection cursor
//! ```
//!
//! The pick is therefore **always for exactly one frame** and its answer is delivered in the same
//! frame the click was pumped in, which is what `WorldPicker::draw_no_blit` reproduces.
//!
//! # The pose the ray sweeps
//!
//! The client sweeps each part at its live position — the frame
//! filled from **this frame's animation**: the part update asks for the current animation frame, leaves every
//! part alone when there is no frame at all, and otherwise combines each part's `pos` from the
//! world frame, the animation frame and the part-array scale.
//!
//! Part drawing — the one place that arms the ray, by storing
//! the current drawn part — draws the part at exactly that position. So the pose the
//! ray sweeps is the **animated** one, and the setup placement is only
//! the no-current-animation fallback; the collision body follows the same rule.
//!
//! Sweeping every **remote** part at that fallback — `placement_frames[0x65]`, falling back to
//! `[0]` — composed with the object's current frame is wrong for anything animated. For a chest
//! or a lifestone the rest pose *is* the current pose and it makes no difference. For a **door**
//! it makes the door permanently closed to the pick: the retail door setup's two leaves swing
//! ~87° and ~1.36 m out of the doorway when it opens, and a rest-pose sweep keeps them standing
//! in it, so a click aimed through an open doorway answers "the door" for ever.
//! `PickScene::object_part_frames` hands the sweep the live pose itself, through
//! `WorldScene::server_object_part_frames`, and the rest pose is only the fallback it is in the
//! client.
//!
//! **Ethereal is *not* what retail tests here.** Setting ethereal writes
//! only the ethereal physics-state bit (mask 4) and a pending-check transient flag, and the two
//! readers of that bit in the client are object-info initialization and
//! tracking collision against the physics object — both collision. Neither
//! physics-part drawing, render-device mesh drawing, nor
//! the selection-ray test looks at it. An open door in retail is still
//! clickable (that is how you close it); what stops it swallowing the doorway is that its leaves
//! have moved.
//!
//! The other half, which is not about animation at all: the rest pose is not assumed to be
//! `0x65`. The server names a placement on a create, on a `0xF748`
//! and on a `0xF749`, so the sweep asks for the id the object is drawn at and the geometry memo is
//! keyed on `(setup, placement)` rather than on the setup alone. Held objects are swept too, for
//! the same reason they are drawn: every drawn mesh is offered to the
//! selection-ray test, children included.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::pick_geometry::{find_object, selection_ray, PickPart, PickPolygon};
use dereth_assets::{Decode, GfxObj, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::shape::Plane;
use dereth_primitives::{DataId, Frame, ObjectId, Vec3, Viewport};

/// What the pick has done, for the log line and for the tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PickStats {
    /// Object-search calls inside the viewport that armed a pick.
    pub requests: u64,
    /// `find_object` calls the viewport test rejected — the unsigned compare of
    /// `(x - viewport.x) < viewport.width && (y - viewport.y) < viewport.height`, which also
    /// rejects negatives and rejects a click above or left of a viewport
    /// that does not start at the window's top-left corner.
    pub outside_viewport: u64,
    /// Picks that ran and found an object.
    pub found: u64,
    /// Picks that ran and found nothing: the ground, the sky, or empty air.
    pub missed: u64,
    /// Parts offered to the selection-ray test.
    pub parts_swept: u64,
    /// Of those, the **local body's**. The client arms the ray test for every
    /// part whose physics-object id is nonzero and has no
    /// player compare, so the player's own parts are offered like anyone else's. Counted apart so
    /// a test can tell "the body was offered and missed" from "the body was never offered".
    pub local_body_parts: u64,
    /// A setup identifier that is not a `0x02000000` setup id. The client wraps a
    /// `0x01000000` graphics-object identifier with a simple setup, and this build has
    /// no simple-setup construction; counted rather than guessed, as `object_physics` does.
    pub setup_not_a_setup: u64,
    /// A setup record that is missing or would not decode. A layout bug, not bad input.
    pub setup_undecodable: u64,
    /// A part whose graphics object has no drawing BSP, so the root-sphere query gave the decoder
    /// nothing to store in `drawing_sphere`. The selection ray first tests that sphere,
    /// so such a part is not pickable in the client either.
    pub parts_without_drawing_sphere: u64,
    /// Candidates dropped because they stand in an **interior** cell the frame's
    /// cell walk never reached — the objects behind the wall, which cell drawing
    /// never offers to the selection ray because
    /// the per-cell draw only ever runs over `cell_draw_list`. Zero on every
    /// outdoor frame and on every scene that reports no cell walk.
    pub objects_in_undrawn_cells: u64,
    /// Candidates dropped because the frame's object pass never **offered** them
    /// to the ray at all — `PickScene::drawn_objects` answered and they are not in it. The case
    /// that matters is an object standing in an **outdoor** cell while the viewer is in an
    /// interior that sees no outdoors: cell drawing takes its
    /// `outside_view.view_count == 0` test and landscape drawing — with every landcell's object list —
    /// never runs. Zero on every scene that reports no draw.
    pub objects_not_offered_this_frame: u64,
    /// Remote parts swept at the **live** pose written this frame — the pose
    /// handed to the device, and therefore the pose the ray sees.
    pub parts_at_the_animated_pose: u64,
    /// Remote parts swept at the setup record's placement frame instead, because the
    /// scene published no live pose for that object. That is the setup-placement fallback:
    /// the client's no-current-animation case for an object that has never been animated —
    /// and, for a harness with no `WorldScene`, "this scene cannot answer". An **animating** object
    /// counted here is swept at the wrong pose: for the retail door that is a closed door standing
    /// in an open doorway.
    pub parts_at_the_rest_pose: u64,
}

/// One part of one setup record, in the part's own space, ready to be swept.
#[derive(Debug)]
struct PartGeometry {
    /// The index into the setup's part array that this geometry was loaded for. The list it sits in skips
    /// parts with no drawable graphics object, so its position is **not** this number; a caller that
    /// has live per-part state (the local body) looks that state up by this.
    setup_index: usize,
    /// The requested placement's frame `[i]`, or placement 0's, through
    /// [`crate::models::placement_frames`] — the pose the object is **currently** drawn at.
    ///
    /// The draw uses a server-named placement, and the memo is keyed on the placement as well
    /// as the setup, so the two cannot disagree. A pick that
    /// swept the wrong pose answers with the wrong part index and, in a crowd, the wrong object,
    /// and nothing on screen shows it — which is why this is a keyed lookup and not a constant.
    rest: Frame,
    /// The setup's default scale for part `[i]`, folded with the object's own `scale` at sweep time exactly as
    /// internal part-array scale setup folds it.
    default_scale: Vec3,
    /// The drawing BSP's root-node sphere.
    drawing_sphere: (Vec3, f32),
    /// Combined with the selection plane, which the dat does not
    /// store. The plane is the physics crate's (`dereth_physics::geom::Polygon`), not recomputed
    /// here.
    polygons: Vec<PickPolygon>,
}

/// One setup record's drawable geometry **at one placement**, memoised.
#[derive(Debug, Default)]
struct SetupPickGeometry {
    parts: Vec<PartGeometry>,
}

/// One of the local body's parts as the part-array draw sees it — the live
/// frame filled by this frame's part update, not the setup's rest pose.
#[derive(Debug, Clone, Copy)]
pub struct LocalBodyPart {
    /// The part's position in the renderer's viewer-block-relative space.
    pub pos: Frame,
    /// The part's graphics-object scale, with the object's `scale` already folded in by
    /// internal part-array scale setup.
    pub gfxobj_scale: Vec3,
    /// `draw_state & 1` — the part draw's first early return, which is
    /// what keeps a first-person body (translucency 1.0 sets the bit) out of the sweep.
    pub no_draw: bool,
}

/// The local player's body, offered to the sweep the way part drawing offers every other
/// drawn part.
///
/// The player has no `SceneObject` (he is `Character`), so [`PickScene::object_frame`] cannot
/// answer for him and the presence sweep skipped him. In the client there is one physics object for
/// the player, registered under the player id. Part drawing, selection-ray testing,
/// the object-found notice and selected-object assignment do not reject the player's id. The one
/// player-id read in the notice handler is the drag arm *accepting* self. So a click
/// on your own body selects you, and this is the data that lets the sweep say so.
#[derive(Debug, Clone)]
pub struct LocalBody {
    /// the server's player id once `0xF746` has named it.
    pub id: ObjectId,
    /// The setup record the body's part array is built from, for the polygon memo.
    pub setup: DataId,
    /// The placement the memo is keyed on. Only the rest-pose fallback reads it; the parts below
    /// carry their own frames.
    pub placement: u32,
    /// Part-array entries, indexed in setup-part order.
    pub parts: Vec<LocalBodyPart>,
}

/// The request flag, stored selection point and clicked-object id used for picking,
/// plus the geometry cache the renderer would have had in hand.
#[derive(Debug, Default)]
pub struct WorldPicker {
    /// The pending object-search flag and stored selection-cursor coordinates.
    /// The coordinates are **viewport-relative**: object search supplies the values produced by
    /// subtracting the viewport origin, not the window coordinates it was called with.
    ///
    /// The two differ because the 3D view sits inside its layout rectangle, not at `(0, 0)`.
    looking_for: Option<(f32, f32)>,
    /// A pending clear of the selected-object-in-view flag until the host applies it.
    ///
    /// This type is handed no
    /// `World`: [`Self::find_object`]'s two production call sites are `Interaction::wrapper_mouse`
    /// and its drop arm, neither of which holds one. So the clear is recorded here and drained by
    /// `Interaction::run_object_range_checks`, which does — the same frame, and **before**
    /// the object-range check reads the flag, which is the ordering that matters: in the
    /// client the click's clear also lands before the next range check.
    ///
    /// Set only on the accepted leg, because the clear is **inside** the two unsigned viewport
    /// comparisons — a click outside the 3D viewport clears the cursor and returns
    /// without touching the flag.
    pending_selected_object_in_view_clear: bool,
    /// The clicked-object identifier latched by the last completed pick.
    click_object_id: ObjectId,
    /// The selection controller's clicked-object index.
    click_object_index: i32,
    /// Keyed on `(setup, placement)`, not on the setup alone: two objects of the same setup posed
    /// differently are two different sets of swept polygons.
    geometry: BTreeMap<(u32, u32), Option<Arc<SetupPickGeometry>>>,
    pub stats: PickStats,
}

/// What the sweep needs from the drawn scene, and nothing more.
///
/// The scene supplies each object and the frame it was drawn at. It is a trait so the sweep can
/// be exercised against real retail geometry with no D3D12 device — the acceptance test replays the
/// packet corpus into an [`crate::objects::ObjectStream`] and supplies the frames itself, which is
/// the same data `dereth_scene::world_scene::WorldScene` would have.
pub trait PickScene {
    /// the camera frame, in the same (viewer-block-relative) space the object
    /// frames are in.
    fn viewer(&self) -> Frame;
    /// The object's achieved position as the renderer placed it this frame.
    fn object_frame(&self, id: ObjectId) -> Option<Frame>;
    /// The field of view used to derive `vdst`, which **must** be the one
    /// the frame was drawn with, or the pick desynchronises from the image.
    ///
    /// The argument is the **back buffer's** extent, not the 3D viewport's: it is what
    /// `dereth_scene::world_scene::WorldScene::view_params` takes, and `view_params` reads the game
    /// viewport's rectangle out of its own field. The name says which of the two rectangles it
    /// is.
    fn fov_y_rad(&self, screen: (u32, u32)) -> f32;
    /// The local player's body, if the scene has one.
    /// Part drawing offers the player's parts to the ray test like any other object's, and he is
    /// the one drawn object [`Self::object_frame`] cannot answer for. `None` is a scene with no
    /// body (a bodyless harness, the character-select viewer), not a hidden one: a hidden body is
    /// a body whose parts all carry `NoDraw`, and those are excluded part by part.
    fn local_body(&self) -> Option<LocalBody> {
        None
    }

    /// The **interior** cells the frame's cell walk
    /// produced, or `None` when the scene has no cell walk to report.
    ///
    /// Object drawing offers a part to the graphics-object selection-ray test
    /// from inside the draw, and indoors the only caller is the per-cell object draw,
    /// run once per entry of the view's
    /// `cell_draw_list` over the objects registered in **that** object cell. So an
    /// object in an interior cell the portal traversal never reached is never offered, and the
    /// set the sweep may consider is the set the cell walk produced — not every object in range.
    ///
    /// `None` is "this scene has no answer", which is a bodyless or never-drawn harness, and it
    /// means *no restriction*; an **empty set** is a real answer ("the walk reached no interior
    /// cell") and restricts every interior object away. The distinction is deliberate:
    /// no scene answer is different from an observed empty set.
    fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
        None
    }

    /// The objects the frame's object pass **offered**, or `None`
    /// when the scene has no draw to report.
    ///
    /// Selection-ray testing is called only from mesh drawing, on both its portal and
    /// non-portal arms, after the view-cone check and before internal mesh submission.
    /// So the candidate set is the frame's **submitted** set, and
    /// [`Self::drawn_cells`] is only half of it: it says which interior cells the portal walk
    /// reached, and says nothing about an **outdoor** object on a frame where landscape
    /// drawing was skipped because `outside_view.view_count == 0`.
    ///
    /// `None` restricts nothing and is a harness that never drew; an **empty set** is a real answer
    /// and restricts everything away, because no scene answer is different from an observed
    /// empty set.
    fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
        None
    }

    /// One drawn object's **live** part frames from the latest part-array
    /// update, indexed in setup-part order.
    ///
    /// This is the remote half of what [`Self::local_body`] already gives for the player, and it is
    /// the pose the ray actually sees: part drawing records the current physics part and
    /// hands the device its frame, then selection-ray testing runs within that submission.
    /// Part updating reads the current animation frame, which
    /// returns the setup's placement frame **only** while no animation is current.
    ///
    /// `None` means "this scene has no live pose for that object", and the sweep then falls back to
    /// the placement frame — which is exactly the client's own fallback, and the right answer for
    /// an object that has never been animated.
    fn object_part_frames(&self, _id: ObjectId) -> Option<Vec<Frame>> {
        None
    }
}

impl WorldPicker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            click_object_index: -1,
            ..Self::default()
        }
    }

    /// Arm picking at the window point `(window_x, window_y)`, as the client does:
    ///
    /// It stores the window point, zeroes `click_object_id` and sets `click_object_index` to
    /// `-1`, subtracts the viewport origin from the point and compares both coordinates
    /// **unsigned** against the viewport width and height. Inside, it hands the *subtracted*
    /// pair to the selection cursor, sets `selected_object_in_view = 0` and
    /// arms the object request and returns true; outside, it
    /// clears the render selection cursor and returns false.
    ///
    /// Viewport setup writes four fields —
    /// the viewport origin followed by
    /// its width and height, after clamping them against the
    /// render target's own width and height.
    ///
    /// **The subtraction is not the identity**: the 3D view sits inside the smart box's layout
    /// rectangle, not at `(0, 0)`. Two things follow and neither is optional:
    ///
    /// * the **compare** is against the rectangle's `width`/`height` *after* the origin is
    ///   subtracted, so a click above or left of the rect wraps negative and is rejected — the
    ///   same unsigned compare that rejects a negative window coordinate;
    /// * the pair handed to the selection cursor, and therefore to picking-ray construction,
    ///   is the **subtracted** one. Field-of-view setup builds
    ///   `pick_ray`'s `tx`/`ty` from `(width - 1) * 0.5 * xinvscale` and
    ///   `(height - 1) * 0.5 * yinvscale`, using the viewport scale fields,
    ///   so the ray is centred on the *rectangle's* centre. Passing the window coordinate would
    ///   offset every click by the rect's origin.
    ///
    /// Returns whether a pick was armed.
    pub fn find_object(&mut self, window_x: i32, window_y: i32, viewport: Viewport) -> bool {
        self.click_object_id = ObjectId(0);
        self.click_object_index = -1;
        // Subtract the viewport origin on signed 32-bit words before the compare. Done
        // in `i64` so the wrap the client gets from `int` overflow cannot be reproduced by
        // accident at a coordinate no window ever produces; `u32::try_from` below is the
        // unsigned compare.
        let vx = i64::from(viewport.x);
        let vy = i64::from(viewport.y);
        let (rx, ry) = (i64::from(window_x) - vx, i64::from(window_y) - vy);
        // The client's compare is on `unsigned long`, so a negative coordinate wraps to a huge
        // value and fails it. That is reproduced, not tidied into a `>= 0` test.
        let inside = u32::try_from(rx).is_ok_and(|x| x < viewport.width)
            && u32::try_from(ry).is_ok_and(|y| y < viewport.height);
        if !inside {
            // The rejected leg clears the selection cursor as its
            // last act before `return false`, through [`Self::clear_selection_cursor`] rather
            // than a copy of its body.
            self.clear_selection_cursor();
            self.stats.outside_viewport += 1;
            return false;
        }
        // The **subtracted** pair is passed to selection, rather than the window coordinates.
        #[allow(clippy::cast_precision_loss)] // a viewport coordinate, at most a few thousand
        let pt = (rx as f32, ry as f32);
        // The selected-object-in-view flag is cleared between storing the cursor and setting
        // the object request. It is recorded here and applied by the host, because this type
        // holds no `World`.
        self.pending_selected_object_in_view_clear = true;
        self.looking_for = Some(pt);
        self.stats.requests += 1;
        true
    }

    /// Whether a pick has cleared the selected-object-in-view flag since this was last
    /// asked. See [`Self::pending_selected_object_in_view_clear`].
    pub fn take_selected_object_in_view_clear(&mut self) -> bool {
        std::mem::replace(&mut self.pending_selected_object_in_view_clear, false)
    }

    /// Whether a pick is armed for this frame.
    #[must_use]
    pub fn looking_for_object(&self) -> bool {
        self.looking_for.is_some()
    }

    /// The stored selection-cursor coordinates that viewpoint updating reads back
    /// to construct the picking ray.
    ///
    /// It is **viewport-relative**, and that is the whole point of exposing it: it is the value
    /// the ray is actually built from, not the window coordinate the caller passed in, so a build
    /// that stored the click and forgot to subtract the origin answers differently here. A test
    /// that asserts the argument instead cannot see that mistake.
    #[must_use]
    pub fn selection_cursor(&self) -> Option<(f32, f32)> {
        self.looking_for
    }

    /// Clear the render selection cursor.
    ///
    /// **The client clears it from exactly two places**: object search's rejected leg — the one
    /// the unsigned viewport compare refuses — and the tail of drawing without blitting.
    ///
    /// It is not the scene-less answer: that is [`Self::draw_no_blit_scene_less`], because
    /// drawing without blitting writes both answer fields before the notice goes out and
    /// `clear_selection_cursor` writes neither.
    ///
    /// It is called from the rejected leg, as in the client. The other retail site
    /// is the no-blit draw's tail, and there both [`Self::draw_no_blit`] and
    /// [`Self::draw_no_blit_scene_less`] clear the request with their own `looking_for.take()`,
    /// which is the same edge; that is a transcription choice and is stated rather than folded.
    pub fn clear_selection_cursor(&mut self) {
        self.looking_for = None;
    }

    /// Apply the pick operation's first three stores and rejection path for a point
    /// this build refuses **before** it reaches the rectangle compare.
    ///
    /// The two stores — `click_object_id = 0` and `click_object_index = -1` — happen **before**
    /// the viewport subtraction and its two unsigned compares, and the rejected leg then
    /// clears the render selection cursor.
    ///
    /// **The two stores are above the compare**, so retail clears the answer for every geometric
    /// hover — including the ones it then refuses for being outside the viewport. This build's
    /// HUD-window refusal returns from `dispatch_ui_hover` *before* calling
    /// [`Self::find_object`] at all, which skips both stores unless this is called: the last
    /// hovered UI item's id would stay in `click_object_id` for as long as the pointer sat on a
    /// HUD window, and the cursor-state update's "found object id is non-zero" test would keep
    /// reading it. With the toolbar arming the targeted cursor, the reticle would show
    /// *incompatible target* over the Use button because the source item was still "found".
    ///
    /// Retail has no second refusal — a pointer over a docked window is simply outside
    /// the game viewport's rectangle — so the clear is not optional here.
    pub fn refuse_find_object(&mut self) {
        self.click_object_id = ObjectId(0);
        self.click_object_index = -1;
        self.clear_selection_cursor();
    }

    /// `click_object_id` and `click_object_index`, as the last completed pick left them.
    #[must_use]
    pub fn click_object(&self) -> (ObjectId, i32) {
        (self.click_object_id, self.click_object_index)
    }

    /// Write the answer for the synchronous object-found notice.
    /// The caller must deliver the returned id before another operation.
    /// This is not find_object: it neither starts nor cancels a geometric pick.
    pub fn set_found_object(&mut self, id: ObjectId, part: i32) -> ObjectId {
        self.click_object_id = id;
        self.click_object_index = part;
        id
    }

    /// The tail of selection drawing without blitting:
    ///
    /// ```text
    /// if an object request is armed:
    ///     click_object_id    = the selected object's identifier
    ///     click_object_index = the selected part's index
    ///     send the object-found notice for click_object_id
    ///     clear the pending object request
    /// clear the render selection cursor
    /// ```
    ///
    /// Returns `Some(id)` when a pick was armed — including `Some(ObjectId(0))`, because the client
    /// raises the notice with a zero id when the ray hit nothing and
    /// the object-found notice handler has a branch for exactly that. `None` means no
    /// pick was requested and no notice is raised.
    ///
    /// There are two size arguments because there are two rectangles. `screen` is the back
    /// buffer, which is all [`PickScene::fov_y_rad`] wants; `viewport` is the render device's
    /// 3D-view rect, whose **width and height** are what
    /// field-of-view setup turns into `pick_ray`'s `tx`/`ty`, centering the ray on that rectangle.
    pub fn draw_no_blit(
        &mut self,
        store: &RetailDatStore,
        scene: &dyn PickScene,
        objects: &crate::objects::ObjectStream,
        screen: (u32, u32),
        viewport: Viewport,
    ) -> Option<ObjectId> {
        let (px, py) = self.looking_for.take()?;
        let viewer = scene.viewer();
        let ray = selection_ray(
            &viewer,
            px,
            py,
            (viewport.width, viewport.height),
            scene.fov_y_rad(screen),
        );

        // **Do not name objects behind interior walls.**
        //
        // The view draws objects once per entry of `cell_draw_list`, using the objects
        // registered in that cell. Indoors, the parts offered to the selection-ray test
        // therefore belong exactly to the drawn cells' objects. Every presence with a position
        // would be every object in the client's tables regardless of the wall between it and
        // the camera.
        //
        // `None` is a scene with no cell walk (a bodyless harness, a frame before the first draw)
        // and restricts nothing; an empty set is a real answer. An object in an **outdoor** cell
        // is not restricted here at all: landscape drawing reaches every in-view landblock's
        // landcells and their objects, which is a
        // different walk from this one.
        let drawn_cells = scene.drawn_cells();

        // **The other half: objects the frame did not submit.**
        //
        // The cell test above is a statement about per-cell object drawing, which runs
        // once per `cell_draw_list` entry, and it deliberately leaves **outdoor** cells alone
        // because landscape drawing is a different walk. The residual is that on an indoor
        // frame that walk **may not run at all**:
        //
        // Indoors, normal-mode rendering takes the inside-draw arm, which never draws
        // the landscape; cell drawing also skips the outdoor walk entirely when
        // `outside_view.view_count == 0`.
        //
        // So the honest test is not "which cells did the walk reach" but "which objects did the
        // frame actually submit", which is the set offered
        // to the graphics-object selection-ray test.
        // [`PickScene::drawn_objects`] publishes exactly that, recorded after the view-cone
        // decision and before the device submission — native's own point, and not the stricter
        // "the GPU accepted the subset".
        //
        // `None` is a scene with no draw and restricts nothing; an empty set is a real answer. The
        // cell test is kept as well: it is the cheaper statement and it holds on the frames where
        // both apply.
        let drawn_objects = scene.drawn_objects();
        // The candidate list, built the way object drawing would have offered it.
        // Held as owned polygon vectors' borrows, so the `Arc`s must outlive the sweep.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        let mut held: Vec<(
            ObjectId,
            Arc<SetupPickGeometry>,
            Frame,
            f32,
            Option<Vec<Frame>>,
        )> = Vec::new();
        for (id, p) in objects.presences() {
            // Drawing accepts a physics object with a nonzero id or an object in creature mode. Every object
            // in this table has a server id, so what excludes a candidate here is the same thing
            // that keeps it out of the draw: an object in a container, which has neither a
            // position nor a holder. A **held** object is drawn — object drawing walks it like
            // any other and drawing offers every mesh to the selection-ray test — so it is not
            // excluded here. The frame the scene hands back is the composed one.
            if p.position.is_none() && p.parent.is_none() {
                continue;
            }
            // The cell walk's answer, for an object that stands in an interior cell.
            // A **held** object has no cell of its own (assigning its parent
            // calls `leave_world`) and is drawn by whichever walk reached its holder,
            // so it is not tested here.
            if let (Some(seen), Some(pos)) = (drawn_cells.as_ref(), p.position) {
                if !dereth_physics::landdefs::is_outdoors(pos.cell) && !seen.contains(&pos.cell.0) {
                    self.stats.objects_in_undrawn_cells += 1;
                    continue;
                }
            }
            // And the frame's own answer, which covers the held and outdoor cases the
            // cell test above deliberately does not. A held object is drawn by whichever walk
            // reached its holder and appears here under its own id, exactly as
            // object drawing offers it.
            if let Some(seen) = drawn_objects.as_ref() {
                if !seen.contains(&id) {
                    self.stats.objects_not_offered_this_frame += 1;
                    continue;
                }
            }
            let Some(setup) = p.setup_id else { continue };
            let Some(frame) = scene.object_frame(id) else {
                continue;
            };
            let Some(g) = self.setup_geometry(store, setup.0, p.placement) else {
                continue;
            };
            // The live part positions for this object, if the scene keeps them. The sweep
            // below prefers them over the memo's placement frame, exactly as
            // part drawing uses the positions written by the part-array update.
            let live = scene.object_part_frames(id);
            held.push((id, g, frame, p.scale, live));
        }

        // **The local body.** The player's presence is in the table above (his
        // `0xF745` arrives like everyone's) but `object_frame` answers `None` for him because he
        // has no `SceneObject`: `Character` draws him. The client does not
        // distinguish the player: a nonzero physics-object id or creature mode is enough, with
        // no player comparison, so his parts are offered
        // here from the body's own part array, at the **live** frames
        // filled by the part-array update, which is the pose the client sweeps (see the module
        // note on the pose the ray sweeps). Guarded against a scene that did give him a
        // `SceneObject`, so there is never a second copy of the same body in the candidate list.
        let candidate_body = scene
            .local_body()
            .filter(|b| b.id.0 != 0 && !held.iter().any(|h| h.0 == b.id));
        // The body goes through the same gate as anyone else.
        // `WorldScene::draw_object_pass` enters him under `Character::object_id`, which is also
        // the physics object's id; viewer-distance updating compares that id
        // against the player id, and the drawing path has no player
        // compare — a frame that submitted none of his parts offered none of them to
        // the selection-ray test either.
        let candidate_body = match (candidate_body, drawn_objects.as_ref()) {
            (Some(b), Some(seen)) if !seen.contains(&b.id) => {
                self.stats.objects_not_offered_this_frame += 1;
                None
            }
            (b, _) => b,
        };
        let body = candidate_body.and_then(|b| {
            let g = self.setup_geometry(store, b.setup.0, b.placement)?;
            Some((b, g))
        });

        let mut parts: Vec<PickPart<'_>> = Vec::new();
        if let Some((b, g)) = &body {
            for part in &g.parts {
                let Some(live) = b.parts.get(part.setup_index) else {
                    continue;
                };
                // Part drawing first checks the NoDraw bit. A first-person body has that
                // bit on every part and offers nothing to the selection ray.
                if live.no_draw {
                    continue;
                }
                parts.push(PickPart {
                    physobj_id: b.id,
                    physobj_index: i32::try_from(part.setup_index).unwrap_or(-1),
                    frame: live.pos,
                    gfxobj_scale: live.gfxobj_scale,
                    drawing_sphere: part.drawing_sphere,
                    polygons: &part.polygons,
                    check_polys: true,
                });
            }
            self.stats.local_body_parts += parts.len() as u64;
        }
        for (id, g, frame, scale, live) in &held {
            for part in &g.parts {
                // Part-array scaling folds the object's `scale` into every
                // part's `gfxobj_scale`; the setup's own `default_scale` is the other factor.
                let s = Vec3::new(
                    part.default_scale.x * scale,
                    part.default_scale.y * scale,
                    part.default_scale.z * scale,
                );
                // The live part position if the scene has it, the setup placement
                // frame if it does not — the client's own two arms, in the client's own order.
                // `g.parts` skips parts with no drawable graphics object, so the live list is indexed by
                // `setup_index` and never by this loop's position.
                let posed = live.as_ref().and_then(|v| v.get(part.setup_index).copied());
                if posed.is_some() {
                    self.stats.parts_at_the_animated_pose += 1;
                } else {
                    self.stats.parts_at_the_rest_pose += 1;
                }
                parts.push(PickPart {
                    physobj_id: *id,
                    // The renderer's selected-part-index query answers with
                    // the part's original index in the setup's part array,
                    // not its position in a list that has skipped the
                    // undrawable ones.
                    physobj_index: i32::try_from(part.setup_index).unwrap_or(-1),
                    frame: posed
                        .unwrap_or_else(|| dereth_physics::math::combine(frame, &part.rest)),
                    gfxobj_scale: s,
                    drawing_sphere: part.drawing_sphere,
                    polygons: &part.polygons,
                    // Object search always enables polygon testing.
                    check_polys: true,
                });
            }
        }
        self.stats.parts_swept += parts.len() as u64;
        let data = find_object(viewer.origin, ray, &parts);
        let (id, index) = data.result();
        self.click_object_id = id;
        self.click_object_index = index;
        if id.0 == 0 {
            self.stats.missed += 1;
        } else {
            self.stats.found += 1;
        }
        Some(id)
    }

    /// The same read-out when the frame had **no scene to sweep**.
    ///
    /// The draw-completion notice block sits outside the has-a-player
    /// guard, so it is reached either way. So a scene-less
    /// frame still answers an armed pick, and it answers with **zero in both fields**:
    ///
    /// It reads the selected part and object IDs, stores them as `click_object_index` /
    /// `click_object_id`, and raises the click event
    /// object-found notice carrying `id`.
    ///
    /// Both getters have the identical shape — the part index is the polygon hit's part when a
    /// polygon was found, else the sphere hit's part when a sphere was found, else 0, and the
    /// object id is the same over its own pair — and clearing the selection cursor
    /// blanks *both* found flags
    /// at the end of every frame. Nothing swept this one, so both answer **0**.
    ///
    /// So [`Self::find_object`] leaves `click_object_index` at `-1` and the client overwrites it
    /// with `0` here. Nothing in this build reads the index while the id is zero, so the
    /// difference is inert today — which is exactly why it is written at the site rather than
    /// left to be re-derived.
    ///
    /// Returns `Some(ObjectId(0))` when a pick was armed, `None` when none was — the same contract
    /// as [`Self::draw_no_blit`], so the caller's notice arm is the same shape either way.
    pub fn draw_no_blit_scene_less(&mut self) -> Option<ObjectId> {
        self.looking_for.take()?;
        self.click_object_id = ObjectId(0);
        self.click_object_index = 0;
        Some(ObjectId(0))
    }

    /// One setup record's drawable geometry at one placement, memoised. `None` for anything that will
    /// not produce one.
    fn setup_geometry(
        &mut self,
        store: &RetailDatStore,
        id: u32,
        placement: u32,
    ) -> Option<Arc<SetupPickGeometry>> {
        if let Some(hit) = self.geometry.get(&(id, placement)) {
            return hit.clone();
        }
        let loaded = self.load_setup_geometry(store, id, placement);
        self.geometry.insert((id, placement), loaded.clone());
        loaded
    }

    fn load_setup_geometry(
        &mut self,
        store: &RetailDatStore,
        id: u32,
        placement: u32,
    ) -> Option<Arc<SetupPickGeometry>> {
        if id >> 24 != 0x02 {
            self.stats.setup_not_a_setup += 1;
            return None;
        }
        let did = DataId(id);
        let setup = match store
            .read_typed(DbType::Setup, did)
            .ok()
            .and_then(|b| Setup::decode_payload_in(store.era_of(did), did, &b).ok())
        {
            Some(s) => s,
            None => {
                self.stats.setup_undecodable += 1;
                return None;
            }
        };
        // Part-array setup places every part at ACE's `Placement.Resting` value `0x65`,
        // falling back to key `0`. A setup with neither leaves the
        // parts at the identity, which is what an absent entry is. Asking for key `0` directly
        // would sweep a pose that is neither the drawn one nor the client's. And `0x65` is only
        // the *installed* pose — the server names another on a create, a `0xF748` or a
        // `0xF749`, so the id the draw used is passed in.
        let rest = crate::models::placement_frames(&setup, placement);
        let mut parts = Vec::with_capacity(setup.parts.len());
        for (i, part) in setup.parts.iter().enumerate() {
            let Some(g) = self.gfx_obj_pick_geometry(store, *part) else {
                continue;
            };
            let rest_frame = rest.and_then(|f| f.get(i).copied()).unwrap_or_default();
            let scale = setup
                .default_scale
                .as_ref()
                .and_then(|s| s.get(i).copied())
                .unwrap_or(Vec3::new(1.0, 1.0, 1.0));
            parts.push(PartGeometry {
                setup_index: i,
                rest: rest_frame,
                default_scale: scale,
                drawing_sphere: g.0,
                polygons: g.1,
            });
        }
        Some(Arc::new(SetupPickGeometry { parts }))
    }

    /// Decode the graphics object's drawing sphere and polygons.
    ///
    /// The polygon loop walks the **raw polygon array**, ignoring the mesh subsets, `NoDraw` and
    /// translucency, so nothing is filtered here either.
    fn gfx_obj_pick_geometry(
        &mut self,
        store: &RetailDatStore,
        did: DataId,
    ) -> Option<((Vec3, f32), Vec<PickPolygon>)> {
        let g = store
            .read_typed(DbType::GfxObj, did)
            .ok()
            .and_then(|b| GfxObj::decode_payload_in(store.era_of(did), did, &b).ok())?;
        // The drawing sphere comes from the drawing BSP's root, so
        // a mesh with no drawing BSP has no sphere and the selection ray's first test —
        // its drawing-sphere ray test — never passes.
        //
        // One computation, [`crate::object_physics::drawing_sphere`], not a second copy of the
        // expression: two independent computations of one field could drift with nothing to
        // notice. It has three readers: the pick, the collision setup
        // (`dereth_physics::source::SetupPart::drawing_sphere`) and the draw path's cone
        // (`WorldScene::BakeCache::drawing_sphere`).
        let Some(s) = crate::object_physics::drawing_sphere(&g) else {
            self.stats.parts_without_drawing_sphere += 1;
            return None;
        };
        let verts = &g.vertex_array.vertices;
        let mut polys = Vec::with_capacity(g.polygons.len());
        for p in &g.polygons {
            let mut v = Vec::with_capacity(p.vertex_ids.len());
            for id in &p.vertex_ids {
                let Some(sw) = verts.get(*id as usize) else {
                    break;
                };
                v.push(sw.position);
            }
            if v.len() < 3 {
                continue;
            }
            // The physics crate's plane, not a second transcription of it.
            let plane = dereth_physics::geom::Polygon::new(v.clone()).plane;
            polys.push(PickPolygon {
                plane: Plane {
                    normal: plane.normal,
                    d: plane.d,
                },
                vertices: v,
                sides_type: p.sides_type,
            });
        }
        Some(((s.center, s.radius), polys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE_800X600: Viewport = Viewport {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    };

    #[test]
    fn set_found_object_writes_identity_without_cancelling_an_armed_geometry_pick() {
        // Recording the found object writes the answer and emits its notice; it
        // does not touch the request flag or requested coordinate.
        let mut p = WorldPicker::new();
        assert!(p.find_object(120, 180, WHOLE_800X600));
        let looking_for = p.looking_for;
        assert_eq!(p.set_found_object(ObjectId(42), 3), ObjectId(42));
        assert_eq!(p.click_object(), (ObjectId(42), 3));
        assert_eq!(p.looking_for, looking_for);
        assert_eq!(p.set_found_object(ObjectId(0), -1), ObjectId(0));
        assert_eq!(p.click_object(), (ObjectId(0), -1));
        assert_eq!(p.looking_for, looking_for);
    }

    /// Oracle: object search rejects each viewport-relative coordinate when its unsigned value
    /// is at least the corresponding viewport extent. Both axes use unsigned above-or-equal.
    #[test]
    fn the_viewport_test_is_unsigned_and_rejects_negatives() {
        let mut p = WorldPicker::new();
        assert!(p.find_object(400, 300, WHOLE_800X600));
        assert!(p.looking_for_object());
        assert!(
            !p.find_object(-1, 300, WHOLE_800X600),
            "a negative x wraps and fails the compare"
        );
        assert!(!p.looking_for_object(), "and clears the request");
        assert!(!p.find_object(400, -1, WHOLE_800X600));
        assert!(
            !p.find_object(800, 300, WHOLE_800X600),
            "the compare is strict"
        );
        assert!(!p.find_object(400, 600, WHOLE_800X600));
        assert!(p.find_object(799, 599, WHOLE_800X600));
        assert_eq!(p.stats.requests, 2);
        assert_eq!(p.stats.outside_viewport, 4);
    }

    /// The origin is subtracted before the unsigned compare.
    #[test]
    fn the_origin_is_subtracted_before_the_unsigned_compare() {
        let rect = Viewport {
            x: 200,
            y: 150,
            width: 400,
            height: 300,
        };
        let mut p = WorldPicker::new();

        // Inside the rect, and *not* at the window's centre.
        assert!(
            p.find_object(400, 300, rect),
            "the rect's centre is inside it"
        );
        assert_eq!(
            p.looking_for,
            Some((200.0, 150.0)),
            "and it is the rect's centre, not (400,300)"
        );

        // The four edges of the rect, in window coordinates.
        assert!(
            p.find_object(200, 150, rect),
            "the rect's own top-left corner is inside"
        );
        assert_eq!(p.looking_for, Some((0.0, 0.0)));
        assert!(
            p.find_object(599, 449, rect),
            "the last pixel of the rect is inside"
        );
        assert_eq!(p.looking_for, Some((399.0, 299.0)));

        // **Above and to the left of the rect.** Before F20 these passed `x < width` outright and
        // armed a pick that the ray then aimed somewhere the player never clicked.
        assert!(
            !p.find_object(199, 300, rect),
            "one pixel left of the rect wraps negative"
        );
        assert!(
            !p.find_object(400, 149, rect),
            "one pixel above the rect wraps negative"
        );
        // Past the far edge, which the old code also rejected -- but at 800/600, not at 600/450.
        assert!(
            !p.find_object(600, 300, rect),
            "one pixel right of the rect"
        );
        assert!(!p.find_object(400, 450, rect), "one pixel below the rect");
        // Inside the *window* but outside the rect on both axes at once.
        assert!(
            !p.find_object(799, 599, rect),
            "the window's last pixel is not the view's"
        );

        assert_eq!(p.stats.requests, 3);
        assert_eq!(p.stats.outside_viewport, 5);
    }

    /// Only an accepted pick clears the in view latch.
    #[test]
    fn only_an_accepted_pick_clears_the_in_view_latch() {
        let mut p = WorldPicker::new();
        assert!(
            !p.take_selected_object_in_view_clear(),
            "nothing owed before any request"
        );

        assert!(
            !p.find_object(-1, 300, WHOLE_800X600),
            "rejected by the unsigned compare"
        );
        assert!(
            !p.take_selected_object_in_view_clear(),
            "the clear is inside the accepted branch, so a rejected click clears nothing"
        );

        assert!(p.find_object(400, 300, WHOLE_800X600));
        assert!(
            p.take_selected_object_in_view_clear(),
            "an accepted pick owes the clear"
        );
        assert!(
            !p.take_selected_object_in_view_clear(),
            "and it is drained, not sticky"
        );
    }

    /// Oracle: the same function's first two lines — `click_object_id = 0; click_object_index = -1`
    /// on **every** request, before the viewport test.
    #[test]
    fn a_new_request_clears_the_previous_answer() {
        let mut p = WorldPicker::new();
        p.click_object_id = ObjectId(0x1234);
        p.click_object_index = 3;
        assert!(
            !p.find_object(-5, -5, WHOLE_800X600),
            "even a rejected request clears it"
        );
        assert_eq!(p.click_object(), (ObjectId(0), -1));
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_sweep_places_parts_at_the_resting_pose_the_draw_uses() {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        for id in [0x0200_0183u32, 0x0200_03B5] {
            let did = DataId(id);
            let bytes = store.read_typed(DbType::Setup, did).expect("reads");
            let setup = Setup::decode_payload(did, &bytes).expect("decodes");
            let want =
                crate::models::placement_frames(&setup, crate::models::PLACEMENT_RESTING).unwrap();
            assert!(
                want.iter().any(|f| *f != Frame::default()),
                "{did} would prove nothing"
            );

            let mut p = WorldPicker::new();
            let g = p
                .setup_geometry(&store, id, crate::models::PLACEMENT_RESTING)
                .expect("has pick geometry");
            // A part with no drawing sphere is dropped, which would shift the indices;
            // neither of these two loses one, so asserting the length keeps the pairing honest.
            assert_eq!(g.parts.len(), setup.parts.len(), "{did} dropped a part");
            for (i, part) in g.parts.iter().enumerate() {
                assert_eq!(part.rest, want[i], "{did} part {i}");
            }
        }
    }
}
