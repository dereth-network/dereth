//! Viewport input, picking, selection, and world tooltips.

use super::*;

impl Interaction {
    /// The render-device viewport rectangle — the origin and extent
    /// that object picking subtracts and compares against.
    ///
    /// Pushed in by `App::ui_use_time` / `App::interaction_use_time` from `<SBOX>`'s own screen
    /// box, the same value `Renderer::set_game_viewport` installs, so the rectangle the pick
    /// measures against and the rectangle the scene is drawn into cannot disagree. `None` hands
    /// the whole back buffer back, which is what every headless harness and every pre-gameplay
    /// phase has.
    pub fn note_game_viewport(&mut self, viewport: Option<Viewport>) {
        self.game_viewport = viewport;
    }

    /// Whether the pointer is over the 3-D view rather than over a HUD window
    /// drawn on top of it — the game-viewport calculation's subtraction, which this
    /// build's `<SBOX>`-box viewport does not carry. Pushed in by
    /// `crate::app::App::pointer_over_game_view`, which is where the element tree is, once a
    /// frame, immediately before the step that consumes it — the same seam
    /// [`Self::note_examine_panel_open`] uses and for the same reason.
    ///
    /// Defaults to **true**, which is "there is no HUD over the viewport": a `--no-ui` run and
    /// every headless harness with no shell, where the render device's viewport is the whole back
    /// buffer and `compute_game_viewport`'s own answer with nothing docked is the same rectangle.
    pub fn note_pointer_over_game_view(&mut self, over: bool) {
        self.pointer_over_game_view = StartsTrue(over);
    }

    /// The live input-manager pointer position, as read by the object-found notice.
    #[must_use]
    pub fn cursor(&self) -> (i32, i32) {
        self.cursor
    }

    /// The rectangle [`Self::game_viewport`] resolves to at a given back-buffer extent.
    ///
    /// Not a plain getter: `None` is the client's "the viewport is the render target", which is
    /// the state the render device is left in before anything docks, and
    /// resolving it here is what keeps `find_object`'s two `sub`s the identity in a headless run.
    pub(super) fn device_viewport(&self, screen: (u32, u32)) -> Viewport {
        self.game_viewport.unwrap_or(Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        })
    }

    /// App's synchronous mouse callback. Keep the source's pick arm before the screen's
    /// request, but leave geometric completion at draw_no_blit. Do not queue it a second time.
    ///
    /// `screen` is the back buffer; the 3D viewport's rectangle comes from
    /// [`Self::note_game_viewport`].
    pub fn dispatch_ui_mouse(&mut self, event: UiMouseEvent, screen: (u32, u32)) {
        self.cursor = (event.x, event.y);
        self.wrapper_mouse(event, screen, is_world_click(event.over));
    }

    /// Hover search from the global loop or mouse movement. A click/drop reason wins;
    /// hover never selects or uses an object. Object search has a synchronous UI-item arm.
    /// Retail returns without searching when the reason is at least 1; otherwise
    /// it stores `SearchReason::MouseOver` (1) **before** calling object search.
    ///
    /// **Why a refused search must undo the store** — otherwise the tooltip only starts after
    /// the viewport has been clicked or an item selected. `search_reason` is put back to
    /// `SearchReason::None` in exactly one
    /// place in the whole client, the unconditional tail of the object-found notice — so the store
    /// is only safe while *every* search reaches a notice.
    /// Object picking has one leg that does not: an out-of-bounds unsigned viewport comparison
    /// clears the selection cursor and returns false without arming a search.
    /// With `looking_for_object` left clear, drawing skips the notice and
    /// nothing ever reopens the hover gate. Every later frame's hover turns straight
    /// around, and the only gestures that still reach the notice are two:
    /// a world click (the mouse-down handler gates on `< SearchReason::Examine`, which 1 passes)
    /// and a hover over a UI item, which object search answers synchronously through
    /// the found-object setter.
    ///
    /// **Why this build reaches a leg retail does not.** The input manager's pointer is window-relative and
    /// stored unclamped — the native input-message handler sign-extends two **signed** shorts —
    /// while the rectangle `find_object` measures against is the render device's viewport, which
    /// in this build is `<SBOX>`'s own box. Windows delivers a `WM_MOUSEMOVE` outside the
    /// client area for the whole of any capture, i.e. every drag, and a docked smart box makes the
    /// same coordinate reachable with no capture at all. So the store-then-refuse pair is a live
    /// path here where in retail's default full-window layout it is not.
    ///
    /// The repair is the invariant and not the store: `search_reason` names a search **in
    /// flight**, `find_object`'s `false` means no search was started, and a gesture that never
    /// began must not hold the gate closed. The store stays exactly where the native path puts it —
    /// ahead of the call, so a reentrant search inside it still sees `SearchReason::MouseOver` —
    /// and is undone only on the leg that armed nothing.
    pub fn dispatch_ui_hover(
        &mut self,
        position: (i32, i32),
        item: Option<ObjectId>,
        screen: (u32, u32),
        game: &mut dereth_client_model::World,
        now: ServerTime,
    ) {
        if self.reason >= SearchReason::MouseOver {
            return;
        }
        // The native mouse-coordinate queries read the pointer **live**; writing this field only
        // in [`Self::dispatch_ui_mouse`], i.e. only when a mouse *button* moved, would make the
        // notice's viewport test measure the last click's position and not the pointer's.
        self.cursor = position;
        let before = self.reason;
        self.reason = SearchReason::MouseOver;
        if let Some(item) = item {
            let found = self.pick.set_found_object(item, -1);
            self.on_world_object_found(found, game, now);
        } else {
            self.stats.picks_requested += 1;
            let rect = self.device_viewport(screen);
            // **A pointer over a HUD window.** Object picking measures
            // against the render device's viewport, which
            // has already shrunk by every docked HUD window; a pointer over one of those is
            // outside it and the search is refused. This build's viewport is `<SBOX>`'s raw box
            // and the shipped layout arms no clamp edge, so the rectangle admits the whole window
            // — see [`Self::pointer_over_game_view`]. The refusal is spelled the same way the
            // rectangle's own refusal is, one branch below, so a HUD-covered pointer produces no
            // pick, no notice and therefore no 3-D tooltip, and leaves no gesture in flight.
            if !self.pointer_over_game_view.0 {
                // The refusal is `find_object`'s, so it has to clear what
                // `find_object` clears: it zeroes `click_object_id` and
                // `click_object_index` **before** the two unsigned viewport checks, and clears
                // the selection cursor on refusal. Returning here without them would leave the last
                // hovered UI item's id standing as the WorldObjects's found object for as long as
                // the pointer was over a HUD window. See [`WorldPicker::refuse_find_object`].
                self.pick.refuse_find_object();
                self.reason = before;
                self.stats.hover_searches_under_the_hud += 1;
                return;
            }
            if !self.pick.find_object(position.0, position.1, rect) {
                // A rejected pick never arms `looking_for_object`, so no draw-time notice arrives
                // to clear `search_reason` again.
                self.reason = before;
                self.stats.hover_searches_not_armed += 1;
            }
        }
    }

    /// Live motion facts at an input callback, before Cast/Combat request predicates read them.
    /// Same authoritative body/fields as the later `use_time` bridge; no motion is advanced here.
    pub fn prepare_ui_dispatch(
        &mut self,
        body: Option<&crate::character::Character>,
        game: &mut dereth_client_model::World,
        screen: (u32, u32),
    ) {
        self.screen = screen;
        self.note_player_physics(body);
        self.bridge_combat_motion(body, game);
    }

    /// Copy changed authoritative motion facts without advancing motion.
    pub(super) fn bridge_combat_motion(
        &mut self,
        body: Option<&crate::character::Character>,
        game: &mut dereth_client_model::World,
    ) {
        if let Some(body) = body {
            let driver = body.driver();
            let state = &driver.movement.interp.interpreted_state;
            let style = state.current_style.0;
            if self.combat_style_bridged != Some(style) {
                self.combat_style_bridged = Some(style);
                game.combat.current_style = style;
                self.stats.combat_style_bridges += 1;
            }
            let forward = state.forward_command.0;
            if self.combat_forward_command_bridged != Some(forward) {
                self.combat_forward_command_bridged = Some(forward);
                game.combat.forward_command = forward;
                self.stats.combat_style_bridges += 1;
            }
        }
    }

    pub fn dispatch_ui_selection_notices(
        &mut self,
        body: Option<&crate::character::Character>,
        objects: &mut crate::objects::ObjectStream,
        now: dereth_primitives::LocalTime,
    ) {
        let origin = body.map(crate::character::Character::position);
        let phys = crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), objects);
        let radius = dereth_client_contract::radar::radar_range(
            origin
                .as_ref()
                .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
        );
        self.run_defender_notifications(&mut objects.world, &phys, radius, now);
        // `auto_target` can raise a second selection notice; retail delivers that reentrantly.
        while self.pending_selection_changes != 0 {
            self.run_selection_change_notices(&mut objects.world, &phys, radius, now);
        }
    }

    /// Tell this frame's `0x1000002B` arm whether `<EXAM>` is on screen, which is
    /// the question the examination action asks the UI element manager.
    ///
    /// Pushed by `App` immediately before [`draw_use_time_with_chat_focus`], so the answer is this frame's UI state
    /// rather than the previous frame's. A caller that never calls this leaves the arm on its
    /// `false` default, which is retail's *"there is no such element"* leg — see
    /// [`Interaction::examine_panel_open`]'s own note about why that is asserted and not assumed.
    pub fn note_examine_panel_open(&mut self, open: bool) {
        self.examine_panel_open = open;
    }

    /// Whether the `0x1000002B` arm took its close-first leg this frame, and clear
    /// the flag.
    ///
    /// `App` drains this immediately after [`draw_use_time_with_chat_focus`] returns and hides the
    /// panel.
    pub fn take_examine_panel_close(&mut self) -> bool {
        std::mem::take(&mut self.examine_panel_close)
    }

    /// **The producer for `place_in_3d`'s `player_on_ground`.**
    ///
    /// The player's physics object is queried for ground contact,
    /// which is `transient_state & 1` **and**
    /// `transient_state & 2` — `CONTACT` and `ON_WALKABLE`, both. Retail
    /// tests contact first, then walkability, returning true only when both bits are set.
    ///
    /// `body` is the local player's physics body; `None` corresponds to an absent body.
    /// [`draw_use_time_with_chat_focus`] supplies `WorldScene::character`, and
    /// [`crate::character::Character::on_ground`] checks the same two bits.
    ///
    /// **Note this is not the command interpreter's logout gate**: that one is
    /// `transient_state & 1` **alone**, so a body in contact with a *non*-walkable
    /// surface may log out and may not drop an item. Two different predicates over one word.
    /// **This once-per-frame lookup also carries pending-motion state.**
    /// Ground contact and combat readiness are different questions, with separate
    /// answers and counters so a mutation to either remains observable. Both
    /// readers use the combat-ready predicate, which never reads
    /// `transient_state`.
    ///
    /// The original readiness decision is:
    ///
    /// * An absent physics body returns false.
    /// * Noncombat and magic return `!motions_pending`.
    /// * Melee first requires a combat maneuver table. With the attack argument
    ///   true, that is sufficient; with false, it also requires `!motions_pending`.
    /// * Missile first requires one of six styles and forward command Ready. With
    ///   the attack argument true, that is sufficient; with false, it also requires
    ///   `!motions_pending`.
    ///
    /// `!motions_pending` alone exactly answers the noncombat/magic arms, but is
    /// merely an upper bound for mode changes in melee or missile: missing weapon
    /// prerequisites can still require queueing the change. It is not a valid attack
    /// answer, because a qualifying melee/missile attack bypasses pending-motion checks;
    /// answering attack-build, execute and power-bar entry from it refuses melee attacks
    /// retail allows.
    ///
    /// [`Self::ready_for_mode_change`] and [`Self::ready_for_attack`] supply the full
    /// decisions. The former serves immediate mode toggling and
    /// pending-mode retry; they both use the false argument.
    ///
    /// The original pending-motion query checks for a movement manager, then asks
    /// its motion interpreter whether the pending list has a head. `Character`
    /// always owns a `MotionDriver`, so this build cannot represent an absent
    /// manager inside an existing body; `movement.motions_pending()` supplies the
    /// answer.
    pub fn note_player_physics(&mut self, body: Option<&crate::character::Character>) {
        let answered = body.map(crate::character::Character::on_ground);
        if answered != self.player_on_ground {
            self.stats.physics_answers_changed += 1;
        }
        self.player_on_ground = answered;

        self.player_motions_pending = body.map(|c| c.driver().movement.motions_pending());
        let ready = self.player_motions_pending == Some(false);
        if ready != self.player_ready {
            self.stats.ready_answers_changed += 1;
        }
        self.player_ready = ready;
    }

    /// Pending-motion state on the local body, `None` when there is no body.
    /// This is the argument the interaction callback takes.
    #[must_use]
    pub fn player_motions_pending(&self) -> Option<bool> {
        self.player_motions_pending
    }

    /// Combat readiness with the false argument, as passed by both mode-change call sites.
    ///
    /// `!motions_pending` alone is the whole of
    /// the non-combat and magic arms and only an **upper bound** on the other two: melee also needs
    /// a combat maneuver table and missile also needs one of the six stances and a `Ready`
    /// `forward_command`, and both of those can only subtract. This is the whole switch.
    #[must_use]
    pub fn ready_for_mode_change(&self, game: &dereth_client_model::World) -> bool {
        game.player_in_ready_position(false, self.player_motions_pending)
    }

    /// Combat readiness with the true argument, as passed by all three attack call sites.
    ///
    /// With the argument set, the melee and missile arms answer
    /// true as soon as the weapon precondition holds, **without** reaching
    /// `motions_pending` -- so this is `true` in states where [`Self::ready_for_mode_change`] is
    /// `false`, and answering the attack sites from the mode-change value refuses swings retail
    /// allows.
    ///
    /// # Missing physics body
    ///
    /// The missing-body rejection is **not** composed here: `motions_pending` is passed as
    /// `Some(false)` rather than `None` when the frame has no body. Everything the readiness
    /// switch decides -- the mode, the combat maneuver table, the six stances and
    /// `forward_command` -- is answered.
    ///
    /// This attack path substitutes no pending motion when no physics-body observation is
    /// available. The mode-change path above instead retains the missing-body rejection.
    /// A device-free presentation can supply a real body; absence here is an observation
    /// state, not a property of all headless frames.
    #[must_use]
    pub fn ready_for_attack(&self, game: &dereth_client_model::World) -> bool {
        game.player_in_ready_position(true, self.player_motions_pending.or(Some(false)))
    }

    /// What [`Self::note_player_physics`] last read for
    /// the pending-motion portion of combat readiness.
    #[must_use]
    pub fn player_ready(&self) -> bool {
        self.player_ready
    }

    /// What [`Self::note_player_physics`] last read, for the tests: `Some(false)` is an airborne
    /// body and `None` is no body at all, which `place_in_3d` treats alike and the client does too.
    #[must_use]
    pub fn player_on_ground(&self) -> Option<bool> {
        self.player_on_ground
    }

    /// The [`Request`]s produced but not yet handed to the wire — what `run`'s step 4 drains.
    ///
    /// The outbox is where "the arm ran" and "the arm sent the right thing" are
    /// distinguishable.
    #[must_use]
    pub fn pending_requests(&self) -> &[Request] {
        &self.outbox
    }

    /// The same, drained — what [`draw_use_time_with_chat_focus`]'s step 4 does to it.
    ///
    /// A test that drives several frames needs "what did *this* frame send", and comparing
    /// lengths across frames answers a different question badly: a frame that sent one message
    /// and a frame that sent one after another sent one look identical in a suffix.
    pub fn take_pending_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.outbox)
    }

    /// Current target mode, for the tests and the cursor.
    #[must_use]
    pub fn target_mode(&self) -> TargetMode {
        self.target_mode
    }

    /// Target-mode assignment changes the leave flag only when the mode changes.
    /// Use -> UseTarget cancels the pending leave, but rearming the same mode does
    /// not. The retained use-source id is separate and survives even Escape/cancel.
    pub(super) fn set_target_mode(&mut self, mode: TargetMode) {
        if self.target_mode != mode {
            self.target_mode = mode;
            self.leave_target_mode = false;
        }
    }

    /// `search_reason`, for the tests.
    #[must_use]
    pub fn search_reason(&self) -> SearchReason {
        self.reason
    }

    /// Viewport mouse-down and mouse-up handling, plus
    /// the toolbar's four stance ids.
    ///
    /// The wrapper's table:
    ///
    /// | edge | button | behaviour |
    /// |---|---:|---|
    /// | down | 7 | the select-left handler with `true`; if it did not consume and `search_reason < SearchReason::Examine`: a target mode up → `SearchReason::TargetedUse`, else `SearchReason::Select`; object search at `(x, y)` |
    /// | down | 8 | `Input.UseMouseTurning` → enable mouse look |
    /// | down | 10 | mouse-look off and `search_reason < SearchReason::Use` → `SearchReason::Use`, object search |
    /// | up | 7 | the select-left handler with `false` |
    /// | up | 8 | a movement drag ends it; else mouse turning → mouse look off; else `search_reason < SearchReason::Use` → `SearchReason::Examine`, object search |
    ///
    /// So with the default `Input.UseMouseTurning = false`, **right-click is examine** and
    /// left-double-click is use. That is not rebindable; it is hard-coded.
    ///
    /// Returns whether a pick was armed.
    ///
    /// **Public only so that a test can call an input into it** — the same
    /// reason and the same precedent as [`Self::on_world_object_found`] above. Its one
    /// production caller is [`draw_use_time_with_chat_focus`] step 1, and every reason this table parks is consumed by
    /// step 3 **in the same frame**, because drawing raises the notice
    /// unconditionally once `looking_for_object` is up. So `search_reason` is `SearchReason::None`
    /// at every frame boundary and a harness that reads it between frames is reading the tail of
    /// the notice, never the gate. Driving this apart from step 3 is the only way to observe the
    /// gate itself: the field a whole-frame test reads is not the field the gate reads.
    ///
    /// `screen` is the back buffer. The 3D viewport's rectangle — the one
    /// object picking subtracts its origin from — comes from
    /// [`Self::note_game_viewport`]; with nothing pushed in, the two are the
    /// same rectangle.
    pub fn wrapper_mouse(
        &mut self,
        e: UiMouseEvent,
        screen: (u32, u32),
        world_click: bool,
    ) -> bool {
        use crate::actions::ui as action;
        // ---- UI action cases 7 and 8 -------------
        //
        // Retail tests the action's start flag, then the current target mode. A
        // press with a nonzero mode sets the leave-target-mode flag before dispatching to the
        // UI manager's action handler; an absent manager returns false.
        //
        // The start flag belongs to the action, and this is not a vendor gate: the fields are
        // target mode and the leave-target-mode flag, which the target-mode setter independently
        // confirms.
        //
        // So this is not a vendor arm at all. It is the **one-shot** half of the use/examine
        // cursor: a click while a target mode is up marks the mode to be left, and
        // the per-frame UI update drops it at the end of the frame
        // (if the flag is set and a target mode is up, it clears the mode, unregisters the
        // targeting input map and updates the cursor; the flag is then cleared) — **whether or not the click hit
        // anything**. Without it the mode would be cleared only by
        // [`Self::execute_target_mode_for_item`], i.e. only by a click that found an object, so a
        // use-cursor armed and then clicked at the sky would stay armed for ever.
        //
        // `0x07`/`0x08` are the left and right mouse buttons — `crate::actions::ui` names the
        // same two ids `PRIMARY_CLICK`/`SECONDARY_CLICK` and the device input's Keystone
        // suppression list is `[7, 8, 10, 11]`. This arm runs **above** everything else in this
        // function because in retail it runs before the viewport wrapper ever sees the
        // click.
        if e.start
            && (dereth_client_contract::actions::ActionId(e.action)
                == dereth_client_contract::actions::mapped::SELECT_LEFT
                || dereth_client_contract::actions::ActionId(e.action)
                    == dereth_client_contract::actions::mapped::SELECT_RIGHT)
            && self.target_mode != TargetMode::None
        {
            self.leave_target_mode = true;
            self.stats.leave_target_mode_armed += 1;
        }
        // The toolbar's stance icons, which are ordinary buttons and not the viewport.
        // Button release handling tests `action == 7 || action == 10`, so a
        // double-click's second release is a click too.
        let click = e.action == action::PRIMARY_CLICK || e.action == 0x0A;
        if e.start && click {
            self.left_pressed_on = e.over;
        } else if click {
            let pressed = self.left_pressed_on.take();
            if e.over.is_some_and(is_combat_mode_button) && pressed == e.over {
                self.stats.combat_mode_toggles += 1;
                self.toggle_combat_mode();
                return false;
            }
        }
        // Mouse-down case 8 turns mouse-look on when mouse turning is enabled, and in
        // this rebuild the answer to that test is *yes*: the host's camera input turns the camera
        // on a held right button unconditionally, at the window-system level and outside this
        // wrapper. The press point is recorded here — **above** the viewport guard, so a release
        // that lands on the HUD cannot leave a stale one behind for the next gesture.
        let right_press = if e.action == action::SECONDARY_CLICK {
            if e.start {
                self.right_pressed_at = Some((e.x, e.y));
                None
            } else {
                self.right_pressed_at.take()
            }
        } else {
            None
        };
        if !world_click {
            return false;
        }
        let rect = self.device_viewport(screen);
        let arm = |me: &mut Self, r: SearchReason| -> bool {
            me.reason = r;
            me.stats.picks_requested += 1;
            me.pick.find_object(e.x, e.y, rect)
        };
        if e.start {
            match e.action {
                // The select-left handler consumes the press only while mouse turning is on
                // *and* mouse-look is active — the classic "hold both buttons to run". Neither is
                // reachable in this build (`Input.UseMouseTurning` defaults false and this module
                // does not own the camera's mouse-look), so the press always falls through, which
                // is the retail default path.
                action::PRIMARY_CLICK if self.reason < SearchReason::Examine => {
                    let r = if self.target_mode == TargetMode::None {
                        SearchReason::Select
                    } else {
                        SearchReason::TargetedUse
                    };
                    arm(self, r)
                }
                // `case 10`: the left double-click, gated on mouse-look being off.
                0x0A if self.reason < SearchReason::Use => arm(self, SearchReason::Use),
                _ => false,
            }
        } else {
            match e.action {
                action::SECONDARY_CLICK if self.reason < SearchReason::Examine => {
                    // **Mouse-up `case 8` has three legs and only the third examines**: an active
                    // mouse movement is ended (and the cursor updated); otherwise mouse-look is
                    // turned off; otherwise, when the search reason is below 3, it becomes
                    // `SearchReason::Examine` and an object search starts.
                    //
                    // Both early returns are "the right button was **turning the camera**, so
                    // letting go ends the turn and appraises nothing". The host's camera input
                    // turns the camera on every right-button hold, so without them **every camera
                    // turn would appraise whatever happened to be under the cursor when the button
                    // came up**, one appraisal request per turn.
                    //
                    // The discriminator is the client's own drag threshold: a press and release at
                    // the same spot is a click, a press and release more than
                    // `DRAG_THRESHOLD_SQUARED` apart is a drag. `crate::actions::ui`'s constant is used
                    // rather than a new one because it is the same question
                    // as the native drag-start test.
                    let dragged = right_press.is_some_and(|(px, py)| {
                        let (dx, dy) = (e.x - px, e.y - py);
                        dx * dx + dy * dy > crate::actions::ui::DRAG_THRESHOLD_SQUARED
                    });
                    if dragged {
                        self.stats.mouse_look_releases += 1;
                        false
                    } else {
                        arm(self, SearchReason::Examine)
                    }
                }
                _ => false,
            }
        }
    }

    /// Object-found notice handling, apart from the
    /// highlighting and the tooltip.
    ///
    /// The order below is retail's, and two things in it are easy to get wrong:
    ///
    /// * the **selection happens for every reason except `SearchReason::Drop` and
    ///   `SearchReason::TargetedUse`** — not only for `SearchReason::Select`. Examining or using an
    ///   object selects it as a side effect, which is why the target box follows a right-click;
    /// * `search_reason` is cleared at the **end**, unconditionally, so a pick that found nothing
    ///   still ends the gesture.
    ///
    /// The `SearchReason::Use` arm's guard is the client's: item use on the selected object runs
    /// only when the found object's wielder id is not the player's own id — you cannot
    /// double-click your own equipped sword into a use.
    ///
    /// **Public only so that a test can call an input into it.** Its one
    /// production caller is [`draw_use_time_with_chat_focus`] step 3, behind `WorldPicker::draw_no_blit`, which needs a
    /// loaded `WorldScene` and a rendered frame — so without this no headless test could reach
    /// any arm of this function directly. It **is** the
    /// client's notice handler, so making it callable is what the notice already is.
    pub fn on_world_object_found(
        &mut self,
        id: ObjectId,
        game: &mut dereth_client_model::World,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        let found = id.0 != 0 && game.weenie(id).is_some_and(|w| w.pwd.bitfield & 0x80 == 0);
        if id.0 != 0 && !found {
            // The retail object-found notice rejects absent/hidden weenies by
            // set_found_object(0,-1), whose synchronous zero notice still completes a drop.
            let zero = self.pick.set_found_object(ObjectId(0), -1);
            self.on_world_object_found(zero, game, now);
            return;
        }
        // Whether the object under the cursor *changed*.
        // Latched **here**, at the top, because that is where
        // the client latches it: the value is held locally across everything the reason
        // arms do, and the tooltip block at the tail reads that copy and not the field. Reading
        // the field again at the tail would be the same answer today and would quietly stop being
        // one the moment an arm re-entered the notice.
        let object_under_cursor_changed = self.iid_selected_object != id;
        // The world selection blink. Retail compares the last found id to the
        // new id and tests whether the flip count is nonzero. When both hold, it restores the
        // previous object's lighting and zero the flip count.
        //
        // Every notice takes this — a *hover* over a different object (or over nothing, id 0)
        // while a blink is in flight restores it and stops counting.
        if self.iid_selected_object != id && self.flip_count != 0 {
            self.pending_lighting
                .push((self.iid_selected_object, LightingMode::Restore));
            self.flip_count = 0;
        }
        if found {
            self.stats.objects_found += 1;
            // `SearchReason::Select` and every reason above it, *including*
            // Drop and TargetedUse, light the found object bright and arm the flip:
            //
            // Retail applies high lighting `(0.99, 1.0)`, sets the flip count to 1
            // and the next flip time to the current local time plus 0.2 seconds, then assigns
            // selection `(id, 0)` unless the reason is Drop (5) or TargetedUse (7).
            //
            // So the world blink is bright at once, on the notice — the doll's is not.
            if self.reason >= SearchReason::Select {
                self.pending_lighting.push((id, LightingMode::High));
                self.flip_count = 1;
                self.time_next_flip = now.0 + dereth_animation::parts::SELECTION_FLIP_INTERVAL;
                if self.reason != SearchReason::Drop && self.reason != SearchReason::TargetedUse {
                    game.set_selected_object(Some(id), false, &mut out);
                    self.stats.selections += 1;
                }
            }
            match self.reason {
                SearchReason::Use => {
                    let wielded_by_player = game
                        .weenie(id)
                        .and_then(|w| w.pwd.wielder_id)
                        .is_some_and(|w| Some(w) == game.player);
                    if !wielded_by_player {
                        if let Some(sel) = game.selected {
                            self.use_object(sel, game, &mut req, &mut out, now);
                        }
                    }
                }
                // Examination raises the assess panel's request.
                // **Not** gated by the inventory lock: examination is not an inventory request.
                //
                // This is the whole of examining an object, not only its appraisal request:
                // `0x00C8` alone leaves out the examine-object notice, which
                // records the awaited appraisal id. The appraise-info reply shows the panel only
                // for the awaited id, so without the notice the cursor route would ask the
                // question with nothing listening for the answer.
                // Routing through [`Self::examine_object`] keeps one examination path in this
                // file. Its zero arm is unreachable from here: this arm is inside `if found`, which
                // requires `id.0 != 0` — exactly as it
                // is in retail, whose object-found handler puts the entire block behind a non-zero id.
                SearchReason::Examine => self.examine_object(game, &mut req, id),
                SearchReason::TargetedUse => {
                    self.execute_target_mode_for_item(id, game, &mut req, &mut out, now)
                }
                _ => {}
            }
        }
        if self.reason == SearchReason::Drop {
            // The drop-target check uses `(drop item, found id, 1)`. A found object that is a
            // container takes the item; otherwise it goes on the ground.
            let item = self.drop_item;
            if item.0 != 0 {
                self.place_in_3d(
                    item,
                    if found { Some(id) } else { None },
                    game,
                    &mut req,
                    &mut out,
                    now,
                );
            }
            self.drop_item = ObjectId(0);
        }
        // **The tooltip.** The whole block uses the change flag latched above:
        // *the object under the cursor changed*. A hover that finds the same object again does
        // nothing at all — not a re-set, not a clear — which is what stops the pointer resting
        // on a chest from
        // re-arming (and so re-delaying) the tooltip on every frame-loop pick.
        if object_under_cursor_changed {
            if let Some(call) = self.world_tooltip(id, found, game) {
                self.pending_tooltip = Some(call);
            }
        }
        // The tail assigns the last-found id unconditionally,
        // hover and zero included —
        // then clears the search reason, whatever happened.
        self.iid_selected_object = id;
        self.reason = SearchReason::None;
        self.absorb(game, out, req);
    }

    /// The global loop's first half — the
    /// flip counter — which [`Self::dispatch_ui_hover`] (its second half) left out:
    ///
    /// Retail returns when the flip count is zero or the current time is before
    /// the next flip. Otherwise it increments the count: at 5 it resets it to zero and
    /// restores lighting; below 5 it schedules the next step 0.2 seconds later and applies high
    /// lighting on odd counts or low lighting on even counts to the last found object.
    ///
    /// So after the notice's own bright (count 1): dim at +0.2 s (2), bright at +0.4 (3), dim at
    /// +0.6 (4), restored at +0.8 (5 → 0). **Two bright flashes**, 0.8 s in all: it "blinks
    /// brightly a couple of times". `cur_time` is the local timer's current time.
    pub fn global_loop_lighting(&mut self, cur_time: f64) {
        if self.flip_count == 0 || cur_time < self.time_next_flip {
            return;
        }
        let count = self.flip_count + 1;
        self.flip_count = count;
        let mode = if count < 5 {
            self.time_next_flip = cur_time + dereth_animation::parts::SELECTION_FLIP_INTERVAL;
            if count & 1 != 0 {
                LightingMode::High
            } else {
                LightingMode::Low
            }
        } else {
            self.flip_count = 0;
            LightingMode::Restore
        };
        self.pending_lighting.push((self.iid_selected_object, mode));
    }

    /// The `apply_lighting` calls queued since the last drain, oldest first — App
    /// hands each to `WorldScene::apply_object_lighting`.
    pub fn take_pending_lighting(&mut self) -> Vec<(ObjectId, LightingMode)> {
        std::mem::take(&mut self.pending_lighting)
    }

    /// The selection flip count, for tests.
    #[must_use]
    pub fn selection_flip_count(&self) -> u32 {
        self.flip_count
    }

    /// Which tooltip assignment or clear the object-found notice makes,
    /// or `None` for the one arm that makes neither. See [`WorldTooltip`] for the
    /// native behavior this transcribes.
    ///
    /// `found` is the caller's, and it matches the native object-found predicate: the block's own
    /// test is `id != 0`, but an id whose weenie is absent or `pwd._bitfield & 0x80` never reaches
    /// here — the function's head answered it with `set_found_object(0, -1)` and a re-entrant zero
    /// notice, which is the arm that produces [`WorldTooltip::Clear`].
    fn world_tooltip(
        &mut self,
        id: ObjectId,
        found: bool,
        game: &dereth_client_model::World,
    ) -> Option<WorldTooltip> {
        // Read the tooltip option through the player-option query: bit `0x100`
        // of the options word, default **on**.
        let show = game
            .player_system
            .options
            .get(dereth_client_model::player::option::SHOW_TOOLTIPS);
        if !found || !show {
            self.stats.object_tooltips_cleared += 1;
            return Some(WorldTooltip::Clear);
        }
        // After obtaining the appropriate name with final argument 0, retail tests
        // the wide buffer's length word, 1 for the empty string (the terminator alone). An object
        // with no name leaves the wrapper's tooltip exactly as it was.
        let Some(name) = game
            .weenie(id)
            .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate))
        else {
            self.stats.object_tooltips_unnamed += 1;
            return None;
        };
        if name.is_empty() {
            self.stats.object_tooltips_unnamed += 1;
            return None;
        }
        self.stats.object_tooltips_set += 1;
        Some(WorldTooltip::Set {
            name,
            pointer_in_viewport: self.pointer_in_viewport(),
        })
    }

    /// Input-manager mouse coordinates less the render viewport's x/y origin, each
    /// **unsigned**-compared against the viewport's width and height, so a pointer above or left
    /// of the rectangle wraps negative and fails the same bounds check. Object picking uses the
    /// identical pair of subtractions and unsigned comparisons against the same four fields, so this
    /// asks [`Self::device_viewport`] rather than respelling them.
    fn pointer_in_viewport(&self) -> bool {
        // The other half of the same rectangle: in retail the game-viewport calculation
        // has already taken the docked HUD out of it, so a pointer over a panel fails
        // these compares. Here it does not, and the hit-test order is what carries the fact — see
        // [`Self::note_pointer_over_game_view`]. Belt and braces with the hover gate above,
        // because this is the test the object-found notice actually makes and the drag arm is the one path that
        // starts the tooltip at the mouse without the ordinary hover's last-entered element.
        if !self.pointer_over_game_view.0 {
            return false;
        }
        let v = self.device_viewport(self.screen);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: wrapping subtraction followed by an unsigned bounds comparison *is* the
        // client's test, with no signed comparison anywhere in the native block.
        let (x, y) = (
            (self.cursor.0 - v.x as i32) as u32,
            (self.cursor.1 - v.y as i32) as u32,
        );
        x < v.width && y < v.height
    }

    /// The `set_tooltip`/`clear_tooltip` call this notice made, for
    /// [`crate::app::App`] to apply to the `<SBOX>` element. Taken, so a second drain in the same
    /// frame does not re-apply it — `set_tooltip` is itself edge-guarded
    /// (`==` first), and a repeat that got through would reset a tooltip the
    /// player is reading.
    pub fn take_world_tooltip(&mut self) -> Option<WorldTooltip> {
        self.pending_tooltip.take()
    }
}
