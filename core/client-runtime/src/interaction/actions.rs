//! Mapped actions, selection cycling, and combat input.

use super::*;

impl Interaction {
    /// One selection case of the player-action handler.
    /// The action dispatcher below returns events it did not consume; input dispatch gives the
    /// winning map's callback first refusal, leaving returned actions free for another listener.
    ///
    /// The sixteen arms below all reduce to this shape, and every one of them is written out at
    /// its own call site so that its four shipped arguments can be read against retail's
    /// `case`. What is shared is only the *mechanism*, which is identical in all twenty-six retail
    /// call sites:
    ///
    /// select-next with the arm's `(closer, ignore_current, kind, exclude_own_wielded)` and — if
    /// the selection did not move — select-next again with `(!closer, true, kind, false)`.
    ///
    /// **The retry is where the wrap-around lives**, and it is worth saying why it is not a
    /// no-op. The second call always passes `ignore_current = true`, which sends select-next
    /// down its arm: `closer` is **inverted again** and the reference is planted at the
    /// far sentinel. So `!closer` here becomes `closer` inside, with a reference of `0.0` or
    /// `73728.0` — the nearest or the farthest candidate in radar range, which is exactly the
    /// other end of the cycle. Pressing "next item" on the farthest item selects the nearest.
    ///
    /// **Ten arms wrap, not twelve.**
    /// Ten arms compare the saved and current selection, and
    /// the six arms that do not wrap are the "Closest" family: cases 4, 7, 10,
    /// 0xE, 0x13 and 0x43. They pass `wraps = false` because they already pass
    /// `ignore_current = true` on the first call and are therefore already at an end of the cycle.
    /// The arithmetic checks out against the other number in the same sentence: 6 arms x 1 call +
    /// 10 arms x 2 calls = **26** select-next calls in the player-action handler.
    ///
    /// `use_corpse` is `case 0x13`/`case 0x14`'s tail only:
    ///
    /// Look up the current selection after cycling. If that object exists and its virtual
    /// corpse predicate is true, invoke item use with `(selected id, 1, 0)`.
    ///
    /// Three details of that sequence are easy to get wrong:
    ///
    /// * It reads the selected id **after** the calls, not the id `select_next` returned — which is
    ///   nothing; `select_next` returns `void` and writes the global. So a press that selects
    ///   nothing still uses the *previous* selection if that happens to be a corpse.
    /// * The corpse predicate is virtual and has **zero direct call sites in retail**, so a
    ///   search for direct calls cannot see this line at all.
    /// * the second argument is 1, where every other gesture in this client passes 0. It is read **only**
    ///   inside the targeted-use arm; a corpse is
    ///   `is_useable` and not `is_useable_targeted`, so it takes the use-request branch above
    ///   and the two values agree here. The destination model passes 0 there and is
    ///   correct for this call — but its doc comment says that argument is 1 "only from the plugin
    ///   API and the spell-casting panel", and these two `case`s are a third producer.
    #[allow(clippy::too_many_arguments)]
    fn run_selection_cycle(
        &mut self,
        game: &mut dereth_client_model::World,
        closer: bool,
        ignore_current: bool,
        kind: dereth_client_model::selection::SelectionType,
        exclude_own_wielded: bool,
        wraps: bool,
        use_corpse: bool,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        // The retry compares against the selection captured before this arm runs.
        // Retail reads it at the selection-case dispatch, before that arm's selection calls.
        let before = game.selected;
        if phys.is_empty() {
            self.stats.selection_cycles_without_geometry += 1;
        }
        let lookup = |id| phys.get(id);
        game.select_next(
            closer,
            ignore_current,
            kind,
            exclude_own_wielded,
            &lookup,
            radar_radius,
            &mut out,
        );
        self.stats.selection_cycles += 1;
        if wraps && game.selected == before {
            game.select_next(!closer, true, kind, false, &lookup, radar_radius, &mut out);
            self.stats.selection_cycle_wraps += 1;
        }
        if use_corpse {
            if let Some(sel) = game.selected {
                if game
                    .weenie(sel)
                    .is_some_and(dereth_client_model::Weenie::is_corpse)
                {
                    self.use_object(sel, game, &mut req, &mut out, now);
                    self.stats.selection_corpse_uses += 1;
                }
            }
        }
        self.absorb(game, out, req);
    }

    #[allow(clippy::too_many_arguments)] // existing listener facts plus the actual finish-jump body owner
    pub(super) fn on_actions(
        &mut self,
        events: Vec<crate::actions::Action>,
        game: &mut dereth_client_model::World,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        ready: bool,
        now: dereth_primitives::LocalTime,
        character: Option<&crate::character::Character>,
    ) -> Vec<crate::actions::Action> {
        // The UI selection type, short because the sixteen arms below are a table and
        // the four arguments per row are the part that has to be readable.
        use dereth_client_model::selection::SelectionType as K;

        let mut left = Vec::new();
        let mut req = RecordingRequests::default();
        let srv = ServerTime(now.0);
        for e in events {
            // The combat action ladder, in its own order.
            // `0x1000005A` is answered whatever the mode is; everything else is offered
            // to exactly one of the two sub-handlers, chosen by the combat mode:
            //
            // `0x1000005A` toggles and answers true; otherwise melee and missile modes go to the
            // combat-action handler, magic mode to the magic-action handler, and every other mode
            // answers false.
            //
            // **In peace mode nothing below the toggle runs at all**, and that is not merely
            // belt-and-braces over the input-map registration: retail leaves the *previous*
            // mode's map registered for the frame in which the mode changes, and a plugin can
            // register any map it likes.
            //
            // **Test the positive control too.** A default `World` starts in
            // `NonCombat`, so testing one of the ten combat or twenty-one magic actions
            // without setting the mode merely exercises the false return. A test
            // drives the same
            // event in `NonCombat` and `Melee` and asserts both answers. New action tests
            // must set `combat.combat_mode`; refusal tests also need a reachable positive
            // case, or silence proves nothing.
            if e.id == action::COMBAT_TOGGLE_COMBAT {
                if e.is_start() {
                    self.pending_combat_toggle = true;
                }
                continue;
            }
            let mode = game.combat.combat_mode;
            let handled = match mode {
                dereth_client_model::combat::CombatMode::Melee
                | dereth_client_model::combat::CombatMode::Missile => {
                    self.handle_combat_action(&e, game, &mut req, ready, now)
                }
                dereth_client_model::combat::CombatMode::Magic => self.handle_magic_action(&e),
                dereth_client_model::combat::CombatMode::NonCombat
                | dereth_client_model::combat::CombatMode::Undef => false,
            };
            if handled {
                continue;
            }
            match e.id {
                // ---- no start-flag gate on USE and EXAMINE --------------------------------------
                //
                // The UI action handler owns both actions. Its dispatch subtracts the radar
                // action id, then 7 for USE, then 6 for EXAMINE. A literal-immediate search
                // for the USE id therefore misses the arm.
                //
                // **Neither arm reads the action's start flag.** The only arm of the handler that
                // does is the mouse-button one (7/8), which reads the press flag and then tests
                // for a target mode.
                //
                // Not gating them is therefore **fidelity**, not a deviation. It is
                // observable only on a `Hold`-family binding, which `fire_action_event`
                // releases on key-up. The shipped `ActionMap` binds neither id as a hold,
                // so the shipped keymap sees no difference and a rebound hold behaves as
                // retail's does.
                //
                // ---- examine is a TOGGLE ------------------------------------------------------
                //
                // The examination case first looks up the panel (`0x100005F7`). With no
                // manager it returns handled; with a visible panel it hides it and returns.
                // A missing or hidden panel reaches the examine request instead.
                //
                // So the examine key **shuts an open examine panel** and sends nothing; only a
                // shut panel reaches examination, and therefore only a shut panel produces
                // the `0x00C8 Item_Appraise` on the wire. Always taking the second leg would
                // leave the panel exactly as it was after pressing the key twice.
                //
                // **Three details that matter:**
                //
                // * The close calls the visibility setter, not another panel action.
                // * The visibility read selects **bit 1**, while bit 0 is the mouse-over-top flag.
                //   The right shift is necessary — reading bit 0 would
                //   have made the toggle fire on hover.
                // * The close-first runs **before** the selected id is read, so it fires with nothing
                //   selected at all. Ours does too.
                //
                // **No sibling in this switch has a close-first leg, and that holds over the
                // whole client rather than over this function.** The UI action handler has
                // eight live arms — five dispatched outcomes over the actions from 7, plus `0x7C`
                // and the three chained subtractions:
                //
                // | action | what it does |
                // |---|---|
                // | `0x07` / `0x08` `SelectLeft` / `SelectRight` | on press, mark an active target mode for exit, then forward to the UI manager |
                // | `0x27` `EscapeKey` | cancel targeting or interrupt movement/repeat attack |
                // | `0x55` `CaptureScreenshot` | capture a screenshot |
                // | `0x7B` `ToggleHelp` | send the help request `(0, 0x10000001)` |
                // | `0x7C` `TogglePluginManager` | close if open, otherwise open |
                // | `0x1000001E` `ToggleRadarPanel` | invert radar visibility and broadcast it |
                // | `0x10000025` `USE` | use the selected object, with no close-first step |
                // | `0x1000002B` `SelectionExamine` | this arm |
                //
                // Two of those are toggles by a *different* mechanism (`0x7C` through Keystone,
                // `0x1000001E` through a `bool` member and a notice) and neither goes near
                // element lookup. Element lookup happens **once** in the whole action handler,
                // and the visibility read there is the only one inside it.
                //
                // Object examination has **ten** call sites in ten distinct functions:
                // the paper doll, the examination panel's selection-change receiver,
                // the toolbar, the spellcasting panel, item lists, the viewport's object-found
                // receiver, the Decal plugin API, player actions, item target-mode execution,
                // and this arm. The other **nine** call it outright. So the toggle belongs to the
                // **key**, not to examining.
                //
                // All eight are transcribed: `0x07`/`0x08` is at the head of
                // [`Self::wrapper_mouse`] with the target-mode exit consumer in
                // [`Self::run_leave_target_mode`]; the other five are the arms below this one.
                // The `0x07`/`0x08` arm is not a *vendor* arm: the fields it reads are the target
                // mode, the leave-target-mode flag and the radar-visible flag. See
                // `wrapper_mouse`.
                action::SELECTION_EXAMINE => {
                    if self.examine_panel_open {
                        // Hide the examination panel. `App` performs it, for
                        // the same reason it answers the question: the tree is not reachable from
                        // here. Nothing else in this arm runs — no examination, no request.
                        self.examine_panel_close = true;
                        self.stats.examine_panel_closes += 1;
                    } else {
                        // `0x2B SELECTION_EXAMINE`. The neighboring action
                        // examines the selected id through the same
                        // entry point the cursor and the toolbar button use.
                        //
                        // **No gate on an empty selection**: retail loads the selected id
                        // *unconditionally* and examines whatever it
                        // holds, **including zero** — which is the arm that arms the magnifying
                        // glass. Skipping the call on an empty selection would make pressing `E`
                        // with nothing selected do nothing at all.
                        self.examine_object(game, &mut req, game.selected.unwrap_or_default());
                    }
                }
                // **The pick-up hotkey.**
                //
                // The default key map (DID `0x14000000`, offset `0x207`) binds `DIK_F` to
                // `SelectionPickUp 0x1000002C` in input map `0x10000007 ItemSelectionCommands`,
                // and that map **is** registered at the gameplay input priority (1000). Without
                // this arm the key would resolve, walk all three dispatch stages, match no arm, and
                // be swept into `InputStats::actions_expired`, with nothing on the wire.
                //
                // Retail's arm is case 1, transcribed on
                // `SELECTION_PICK_UP`: the selection gate is retail's own non-zero selected-id test,
                // and `force_main_pack` is its literal `false`, so the destination is the
                // player's preferred pack rather than forced to the main one. `place_in_backpack`
                // is the same production path the toolbar uses.
                //
                // The `break` on no selection is retail's too, and it is load-bearing: the action
                // is left **unconsumed** so a later stage may still take it, rather than being
                // swallowed into a no-op.
                action::SELECTION_PICK_UP => {
                    if let Some(sel) = game.selected.filter(|s| s.0 != 0) {
                        // `req` is this function's own sink, the one `on_actions` drains and
                        // sends. A fresh `RecordingRequests` here would take the `0x0019` and
                        // drop it on the floor: the arm would fire, the counter would climb, and
                        // nothing would reach the wire.
                        let mut out = Notices::default();
                        if game.place_in_backpack(
                            &mut req,
                            &mut out,
                            sel,
                            false,
                            // The selection-changed handler seeds this shared quantity before input.
                            // A fresh `SplitState::default()` is 0/0 and makes auto-merge encode
                            // amount 0, which ACE rejects as "Merge amount not valid!".
                            game.split,
                            ServerTime(now.0),
                        ) {
                            self.stats.pick_ups += 1;
                        }
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // Case 2. Unlike pick-up's
                // no-selection `break`, this arm always sends the current selected id (including
                // zero) and returns true. `App` owns the synchronous notice's toolbar receiver.
                action::SELECTION_SPLIT_STACK => {
                    self.pending_split_stack = Some(game.selected.unwrap_or_default());
                }
                action::USE => {
                    // The native UI use action calls even when the selected id is zero.
                    // Its zero arm enters generic Use rather than silently doing nothing.
                    let mut out = Notices::default();
                    self.use_object(
                        game.selected.unwrap_or_default(),
                        game,
                        &mut req,
                        &mut out,
                        ServerTime(now.0),
                    );
                    self.absorb(game, out, RecordingRequests::default());
                }
                // ---- the player-action handler's four item cases --------------------------
                //
                // Case 0, *Select Self*: with the use-on-target cursor up it puts the cursor away
                // and uses the held item on the player; with the examine cursor up it selects the
                // player, puts the cursor away and examines the player; otherwise it selects the
                // player.
                action::SELECTION_SELF => {
                    if let Some(player) = game.player {
                        let mut out = Notices::default();
                        match self.target_mode {
                            TargetMode::UseTarget => {
                                self.set_target_mode(TargetMode::None);
                                let _ = game.target_acquired(
                                    &mut req,
                                    &mut out,
                                    player,
                                    game.split,
                                    ServerTime(now.0),
                                );
                            }
                            TargetMode::Examine => {
                                game.set_selected_object(Some(player), false, &mut out);
                                self.set_target_mode(TargetMode::None);
                                self.examine_object(game, &mut req, player);
                            }
                            _ => game.set_selected_object(Some(player), false, &mut out),
                        }
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // *Give*: the selected item to the selection before it, which must be a creature
                // or a character; the creature is selected again. Anything else is refused out
                // loud.
                action::SELECTION_GIVE => {
                    let (item, to) = (game.selected.unwrap_or_default(), game.prev_selected);
                    if let Some(to) = to.filter(|t| item.0 != 0 && t.0 != 0 && *t != item) {
                        let mut out = Notices::default();
                        if game.weenie(to).is_some_and(|w| w.is_creature()) {
                            let player_on_ground = self.player_on_ground == Some(true);
                            let _ = game.attempt_place_in_3d(
                                &mut req,
                                &mut out,
                                item,
                                Some(to),
                                false,
                                player_on_ground,
                                game.split,
                                ServerTime(now.0),
                            );
                            game.set_selected_object(Some(to), false, &mut out);
                        } else {
                            out.emit(dereth_client_model::Notice::DisplayString {
                                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                                channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                                text: "You must select a creature or a character to give that \
                                       to.\n"
                                    .into(),
                            });
                        }
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // *Drop*: the selected item on the ground, when the player owns it.
                action::SELECTION_DROP => {
                    if let Some(item) = game.selected.filter(|s| s.0 != 0) {
                        let mut out = Notices::default();
                        if game.is_owned_by_player(item) {
                            self.place_in_3d(
                                item,
                                None,
                                game,
                                &mut req,
                                &mut out,
                                ServerTime(now.0),
                            );
                        } else {
                            out.emit(dereth_client_model::Notice::DisplayString {
                                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                                channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                                text: "You must pick that up first".into(),
                            });
                        }
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // *Move to Main Pack*: the pick-up arm with the main pack forced.
                action::SELECTION_MOVE_TO_MAIN_PACK => {
                    if let Some(sel) = game.selected.filter(|s| s.0 != 0) {
                        let mut out = Notices::default();
                        if game.place_in_backpack(
                            &mut req,
                            &mut out,
                            sel,
                            true,
                            game.split,
                            ServerTime(now.0),
                        ) {
                            self.stats.pick_ups += 1;
                        }
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }

                // ---- the other six UI action arms ---
                //
                // The handler has five outcomes: mouse selection (`0x07`/`0x08`), Escape
                // (`0x27`), screenshot (`0x55`), help (`0x7B`), and `return false` for every other
                // action in its range. `0x7C` is handled separately.
                //
                // **The `SelectLeft`/`SelectRight` arm is not here**: `0x07`/`0x08` are the mouse
                // buttons, and this build routes those through `UiMouseEvent`, so it lives at the
                // head of [`Self::wrapper_mouse`] with its reading written out there.

                // The longest arm in the function, and — with `CaptureScreenshot` —
                // one of the two most likely to be reached in ordinary play. Without it
                // **nothing in the game screen would answer Escape at all**: it would fall through
                // to `_ => left.push(e)` below. Other Escape consumers are
                // pre-game `UiShell::mode_on_action`
                // map-9 handling, chat-entry deactivation, and the text element's
                // `lose_focus_on_escape` flag.
                //
                // Retail's ordering: a jump power strictly above zero finishes the jump and returns
                // true. Otherwise a focused UI element may consume Escape and return true. Then
                // test standing-still and repeat-attack state. If standing still with no repeat
                // attack, clear an active target mode first; otherwise mark a selected target as
                // willingly lost before clearing it, or toggle gameplay options `0x1000001B`
                // when nothing is selected. These paths return true. If moving or repeating an
                // attack, stop completely, print "Action interrupted" on `0x1A` only if moving,
                // recheck repeat-attack state and abort automatic attack if needed, then return false.
                //
                // Four details that decide behaviour:
                //
                // * **The first field is the target mode, not a vendor id**, so the first of the three
                //   cascade legs is *"cancel the use/examine cursor"*, which is what Escape does
                //   in play, rather than anything to do with a vendor.
                // * **The command interpreter's calls** are finish-jump, standing-still and
                //   stop-completely, on the interpreter owned by the viewport.
                // * **The saved movement flag is the *inverse* of the standing-still check**, so the
                //   *"Action interrupted"* line prints when the player was **not**
                //   standing still — and the whole stop-completely leg is taken when they were
                //   not standing still **or** a repeat attack is running.
                // * **The jump-power compare is `> 0.0`.**
                //   Retail takes the second leg on equality or less-than,
                //   so a power of exactly `0.0` is *not* a jump. The jump-power level floors at
                //   `MIN_JUMP_EXTENT` while a jump is pending, so the test is "is a jump pending".
                //
                // **The focus-element leg is structural here rather than reproduced.**
                // `interaction::use_time` is handed only the
                // actions the UI declined (`InputShell::take_events` after `UiShell::frame`), and
                // `dereth_ui`'s focused text element consumes `0x27` and relinquishes focus
                // itself. So an Escape that reaches this arm is by construction an Escape that no
                // focused element wanted — which is exactly the state the native focus gate tests
                // for, and a test asserts it.
                action::ESCAPE_KEY => {
                    self.escape_key(game, &mut req, now, character);
                }

                // The screenshot operation fills the path it is handed and
                // answers a `bool`; on `true` the arm formats `L"Screenshot saved to file '%hs'"`
                //  with it and adds it to the scroll as type `0x1A`, window 0. The arm
                // returns **TRUE either way**, so a failed screenshot still eats the key and
                // prints nothing — which is the one behaviour a player can observe about the
                // failure path.
                //
                // The device lives in `App`, so the request is recorded here and performed there,
                // the same seam as `0x1000002B`'s panel hide.
                action::CAPTURE_SCREENSHOT => {
                    self.screenshot_requested = true;
                    self.stats.screenshots_requested += 1;
                }

                // Open help page `0x10000001`, then return true. `0x10000001` is `ToggleCasPanel`'s
                // id used as a Keystone page selector, not an action raised here.
                //
                // **Keystone is third-party and outside this rebuild's scope**, so
                // what is transcribed is the arm: the key is consumed, the request is counted, and
                // nothing is drawn. That is a smaller effect than retail's and it is *declared*
                // — the alternative is leaving the id to fall through to `_ =>`.
                // `dereth_ui_screens`' `handle_key_press` still carries an empty
                // `action::OPEN_HELP => {}` arm that this one runs above.
                action::TOGGLE_HELP => {
                    self.stats.help_opens += 1;
                }

                // Ask whether the platform plugin manager is open, then close it or open it respectively;
                // return TRUE. **This is a toggle by a completely different mechanism from the
                // examine one** — it asks Keystone rather than the UI element manager, and there
                // is no element, no visibility bit and no visibility setter anywhere in it.
                //
                // Keystone is not rebuilt, so the *state* is held here; the *decision* — ask, then
                // take the opposite branch — is the transcription, and it is asserted over two
                // presses because one press cannot distinguish a toggle from a set.
                action::TOGGLE_PLUGIN_MANAGER => {
                    if self.plugin_manager_open {
                        self.plugin_manager_open = false;
                        self.stats.plugin_manager_closes += 1;
                    } else {
                        self.plugin_manager_open = true;
                        self.stats.plugin_manager_opens += 1;
                    }
                }

                // The third distinct toggle mechanism in the same function: retail
                // inverts the radar-visible flag, broadcasts its new value, and returns true.
                //
                // No element, no element lookup, no visibility bit: a `bool` member and a notice.
                //
                // **And the notice reaches nobody, in retail.**
                // The radar-visibility broadcaster invokes each subscriber's virtual handler.
                // Every implementation of that handler in retail is an empty stub.
                // The radar-visible flag has no known reader beyond startup initialization,
                // session reset and this toggle itself.
                //
                // **So the arm is transcribed as what it is — a flag flip and a notice with no
                // subscriber — and this build does not invent a radar it hides.** Hiding
                // `<RADA>` here would be a behaviour retail does not have.
                action::TOGGLE_RADAR_PANEL => {
                    self.radar_visible = StartsTrue(!self.radar_visible.0);
                    self.stats.radar_visibility_notices += 1;
                }

                // ---- the player-action handler's sixteen selection cases ----------
                //
                // **No `e.start` gate, deliberately.**
                // The player-action handler switches on the action id and never reads the start
                // flag; the combat-action handler does, which is its press/release split.
                // Every shipped binding for these sixteen is
                // `ToggleType::OneShot`, which emits only a `start: true` event, so the gate would
                // be inert on the shipped keymap; a rebound `Hold` would fire the cycle on press
                // *and* release, and that is retail's behaviour, not a defect to be papered over
                // here. (The two arms above do not gate on `e.start` either, for the reason
                // written at `SELECTION_EXAMINE`.)
                //
                // Each tuple is `(closer, ignore_current, kind, exclude_own_wielded)` and is the
                // one its `case` pushes; `wraps` is whether that `case` re-calls when the selected id
                // did not move, and `use_corpse` whether it ends in item use on a selected corpse.
                action::SELECTION_CLOSEST_COMPASS_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::CompassItem,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_COMPASS_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::CompassItem,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_COMPASS_ITEM => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::CompassItem,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                // The one call site of twenty-six that sets the exclude-own-wielded flag -- and
                // the one place it cannot matter: see the constant's doc.
                action::SELECTION_CLOSEST_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::Item,
                    true,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::Item,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_ITEM => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::Item,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_CLOSEST_MONSTER => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::Monster,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_MONSTER => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::Monster,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_MONSTER => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::Monster,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_CLOSEST_PLAYER => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::Player,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_PLAYER => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::Player,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_PLAYER => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::Player,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                // `case 0x13` and `case 0x14` — select, then use the winner if it is a corpse.
                action::SELECTION_USE_CLOSEST_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::UnopenedCorpse,
                    false,
                    false,
                    true,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_USE_NEXT_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::UnopenedCorpse,
                    false,
                    true,
                    true,
                    phys,
                    radar_radius,
                    srv,
                ),
                // `case 0x43` and `case 0x44` — the same two selections, and no use.
                action::SELECTION_CLOSEST_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::UnopenedCorpse,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::UnopenedCorpse,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                // `case 0xD` — the seventeenth player-action
                // selection arm and the only one that is not a `select_next` cycle.
                //
                // The id is read **here as well as inside** `select_last_attacker`, through the
                // shared last-attacker state, because the geometry seam runs the other way from
                // the sixteen above: `select_next` is handed a lookup and asks it per candidate,
                // while the radar-range check is called with an id already in
                // hand and resolves it itself (object lookup, then
                // player-space conversion). `None` here is that function's own
                // null-object / null-cell escape, which returns **false**
                // — so an attacker with no physics object is never re-selected, the opposite of
                // the hearing check's escape. Only `x` and `y`: `z` is not in the sum.
                action::SELECTION_LAST_ATTACKER => {
                    let mut out = Notices::default();
                    let attacker = game.last_attacker();
                    let where_is_he = phys
                        .get(attacker)
                        .map(|p| (p.player_space.0, p.player_space.1));
                    game.select_last_attacker(where_is_he, radar_radius, &mut out);
                    self.stats.selection_last_attacker += 1;
                    self.absorb(game, out, RecordingRequests::default());
                }
                // ---- the player-action handler's two fellowship cases ------------
                //
                // The only two `Selection*` actions in `ItemSelectionCommands` that are not a
                // `select_next` cycle over the world: they walk the membership table and
                // never look at geometry, a radar radius or a selection kind.
                action::SELECTION_NEXT_FELLOW => self.select_fellow(game, true),
                action::SELECTION_PREVIOUS_FELLOW => self.select_fellow(game, false),
                // ---- `case 0x1000002E`, the shipped `P` -------------------------
                //
                // The native arm loads the previous selection, returns false if it is zero,
                // and otherwise assigns that id with `force = 0`.
                //
                // Selection history is modelled: selection assignment writes
                // `prev_selected = old` inside its own `old != id` guard, as retail does
                // — so "previous" means *the value
                // the selected id held immediately before the last change that actually changed it*,
                // a cleared selection included (retail stores the zero here and keeps the last
                // **non**-zero one in separate retained state, which
                // this arm does not read).
                //
                // Because selection assignment then records the outgoing id as the new previous,
                // pressing `P` twice returns to where it started: retail's `P` is a **toggle**
                // between the last two selections, not a stack. That falls out of the two
                // functions rather than being arranged here, and a test asserts it.
                // ---- input map 0x10, the system-key swallow ---------------------
                //
                // The native system-key callback returns true unconditionally — consume, do nothing, and
                // deny the action to the input-handler chain behind it. The decision about what
                // the *keys* do belongs to the platform input-message handler and is already transcribed in
                // the device input's system-key rule; see [`action::SYSTEM_ALT_TAB`] for the
                // four-way answer. This arm exists so that the four rows are handled and the
                // emptiness is a **verified decision** rather than a
                // `_ => {}` nobody has looked at.
                action::SYSTEM_ALT_TAB
                | action::SYSTEM_ALT_ENTER
                | action::SYSTEM_ALT_F4
                | action::SYSTEM_CTRL_SHIFT_ESC => {
                    self.stats.system_keys_swallowed += 1;
                }
                action::SELECTION_PREVIOUS_SELECTION => {
                    if let Some(prev) = game.prev_selected.filter(|id| id.0 != 0) {
                        let mut out = Notices::default();
                        game.set_selected_object(Some(prev), false, &mut out);
                        self.stats.selection_previous_restores += 1;
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // ---- the `PlayerOption_*` actions: each one flips its option -----------------
                //
                // A key bound to one of these reads the option and writes its opposite through
                // the same setter the Character Options page uses, so the change hook, the
                // fellowship exclusions and the save-at-once split all apply. Lock-UI has an
                // action name but no arm, and falls through unhandled.
                id if player_option_action(id.0).is_some() => {
                    if let Some(ordinal) = player_option_action(id.0) {
                        self.toggle_player_option(game, &mut req, ordinal, srv);
                    }
                }
                _ => {
                    left.push(e);
                    continue;
                }
            }
        }
        self.absorb(game, Notices::default(), req);
        left
    }

    /// Select the next or previous fellowship member — the shipped `M` and `N`.
    /// Both walk the membership table and apply the answer with
    /// `force = false`. With no fellowship, neither changes the selection. An empty
    /// table leaves next-selection unchanged but clears previous-selection.
    ///
    /// So, in one sentence each: **next** is the member after the selected one, wrapping to the
    /// first, and the first when the selection is not a fellow; **previous** is the member before
    /// the selected one, wrapping to the last, and the last when the selection is not a fellow.
    /// **Self is included** — the fellow table holds every member, the player among them — and both
    /// directions can therefore land on the player's own object.
    ///
    /// # The one deviation, stated rather than hidden
    ///
    /// The original `PackableHashTable` of fellows iterates bucket `id % bucket_count`,
    /// then that bucket's chain, then the next nonempty bucket.
    /// This build keeps the fellowship in a
    /// `BTreeMap<ObjectId, Fellow>`, so the order is ascending object id. Reproducing the bucket
    /// order would mean reproducing the unpacked bucket count of a table this build never
    /// receives as a table, and it would put `M` in a *different* order from the fellowship panel,
    /// which iterates the same `BTreeMap` (the HUD's fellowship view). The cycle, the
    /// wrap and the two "not a fellow" fallbacks below are retail's; only which member is "next"
    /// within a fixed set differs, and it differs consistently with what the panel shows.
    fn select_fellow(&mut self, game: &mut dereth_client_model::World, next: bool) {
        let Some(f) = game.fellowship.as_ref() else {
            // No fellowship: both directions return without
            // touching the selection.
            return;
        };
        let ids: Vec<dereth_primitives::ObjectId> = f.members.keys().copied().collect();
        let selected = game.selected.filter(|id| id.0 != 0);
        // Test fellowship membership of the *current* selection, which both
        // functions make before they look for it in the walk.
        let at = selected.and_then(|id| ids.iter().position(|m| *m == id));

        let pick = if next {
            match at {
                // The successful next-member search and its fall-through share a fallback: the wrap and the
                // not-a-fellow case are the same statement in retail, and they are here too.
                Some(i) => ids.get(i + 1).or_else(|| ids.first()).copied(),
                None => ids.first().copied(),
            }
        } else {
            match at {
                // Previous-of-first has no preceding key; the walk continues to the end
                // and selects the LAST key, as it does when the selection is not a member.
                Some(0) | None => ids.last().copied(),
                Some(i) => ids.get(i - 1).copied(),
            }
        };

        match pick {
            Some(id) => {
                let mut out = Notices::default();
                game.set_selected_object(Some(id), false, &mut out);
                self.stats.selection_fellow_cycles += 1;
                self.absorb(game, out, RecordingRequests::default());
            }
            None if !next => {
                // The previous-member empty-table leg *clears* the selection,
                // where the next-member empty-table leg returns instead.
                // Faithful, and unreachable on a shard: a fellowship always holds its founder.
                let mut out = Notices::default();
                game.set_selected_object(None, false, &mut out);
                self.stats.selection_fellow_cycles += 1;
                self.absorb(game, out, RecordingRequests::default());
            }
            None => {}
        }
    }

    /// The UI action handler's `EscapeKey` arm.
    ///
    /// The first Escape leg is synchronous with the
    /// input event, before a following jump press/release. App can invoke only this complete
    /// early-return leg; all remaining Escape owners stay in `escape_key`. No start-bit guard
    /// is present in the primary. A focused UI consumer that already ate Escape is not here.
    pub fn try_finish_jump_from_escape(
        &mut self,
        game: &mut dereth_client_model::World,
        now: dereth_primitives::LocalTime,
        character: Option<&crate::character::Character>,
    ) -> bool {
        // Strictly greater than zero.
        if game.combat.jump_power_level(now) > 0.0 {
            crate::jump::finish(&mut game.combat, character);
            self.stats.escape_finish_jumps += 1;
            true
        } else {
            false
        }
    }

    /// The listing and the four details are at the `action::ESCAPE_KEY` call site; this is the
    /// body, in retail's own order. Every leg is `return TRUE` except the last two, which are
    /// `return FALSE` — and the difference is not cosmetic in retail, because a `FALSE` lets the
    /// action reach the next input handler. Here `on_actions` consumes the event either way,
    /// which is the same deviation every other arm in this `match` already carries; the counters
    /// below are what distinguishes the legs for a test.
    fn escape_key(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        now: dereth_primitives::LocalTime,
        character: Option<&crate::character::Character>,
    ) {
        if self.try_finish_jump_from_escape(game, now, character) {
            return;
        }
        // The focused-element leg is structural here; see the call site.
        //
        // Standing still is true without a physics body; otherwise the motion interpreter
        // answers it.
        let standing_still = self.standing_still.0;
        // Repeat-attack state is asked **twice** in retail, once here and again
        // after `stop_completely`. Nothing between the two can change it in this build, so it is
        // read once; the second read is where `abort_automatic_attack` hangs.
        let repeating = game.repeat_attack_in_progress();
        if standing_still && !repeating {
            // Target mode takes precedence over selection and options.
            if self.target_mode != TargetMode::None {
                // Set the target mode to none. Straight to `None`
                // rather than through `leave_target_mode`: setting the target mode also
                // clears that flag directly.
                self.set_target_mode(TargetMode::None);
                self.stats.escape_target_mode_clears += 1;
            } else if game.selected.is_none_or(|id| id.0 == 0) {
                // Escape with nothing selected toggles the gameplay options panel. `App`
                // performs the UI operation; a missing manager consumes the action without
                // opening anything, corresponding here to a request with no host drain.
                self.visibility_toggle_requested = Some(action::TOGGLE_GAMEPLAY_OPTIONS_PANEL.0);
                self.stats.escape_options_toggles += 1;
            } else {
                // Mark the target as willingly lost. **This is the only writer of `true` in
                // the client**, and its reader is the selection-changed notice, which
                // clears it — so this store and the selection-change handler's clear-once are one
                // mechanism: Escape
                // says *"I let go of that target on purpose"*, and the very next selection change
                // is not answered by auto-targeting.
                //
                // Set **before** the selection is cleared, which is what makes the flag visible to the
                // notice that call raises.
                game.combat.target_willingly_lost = true;
                let mut out = Notices::default();
                // Clear the selection with `force = false`.
                game.set_selected_object(None, false, &mut out);
                self.stats.escape_deselects += 1;
                self.absorb(game, out, RecordingRequests::default());
            }
            return;
        }
        // Stop the interpreter completely, including its physics body. `App` owns the body.
        self.stop_completely_requested = true;
        self.stats.escape_stops += 1;
        // Print only when the player was **moving**, on refusal channel `0x1A`.
        if !standing_still {
            self.refuse(game, "Action interrupted");
            self.stats.escape_interrupts += 1;
        }
        // Recheck the cached repeat-attack state and abort any automatic attack.
        if repeating {
            game.abort_automatic_attack(req);
            self.stats.escape_attack_aborts += 1;
        }
    }

    /// Handle combat actions for **melee and missile at once**, which
    /// is the shape of the function and the shape of the feature.
    ///
    /// Retail shares the exact
    /// same keybinds to lower/increase the gauge that scales between SPEED and POWER (melee) or
    /// ACCURACY (missile), as well as the keybinds to initiate an attack with high, medium or low
    /// height: `0x1000005B` (`CombatDecreaseAttackPower`) and
    /// `0x100000EF` (`CombatDecreaseMissileAccuracy`) share a `case`, as do the increase pair and
    /// each of the three heights with its aim twin:
    ///
    /// Retail's press handling dispatches on one range of action suffixes, `0x5B..0xF3`.
    /// Release handling uses a second table, reads the currently requested attack height,
    /// and ends the attack request with that height and power argument `-1.0`.
    ///
    /// Two details, both invisible from the screen:
    ///
    /// 1. **The release uses the requested-attack-height field, not the released key's
    ///    height** — so releasing a *different*
    ///    height key than the one held swings at the height that is set, and does not silently
    ///    change it.
    /// 2. **The press goes through `set_requested_attack_height`**, whose guard is
    ///    `old != new || !attack_request_in_progress` — so a **held** key, which the repeat sweep
    ///    re-delivers every frame, runs the whole thing once and then nothing. What that saves is
    ///    `start_attack_request`'s target check and its `0x1A` refusal, once per frame; it is *not*
    ///    the power bar, which `attempt_start_building_attack` protects on its own. See
    ///    the world's attack-height setter.
    ///
    /// Returns `handle_combat_action`'s own `bool`, which is what the action callback returns and
    /// therefore
    /// what decides whether the event is offered to the next listener.
    fn handle_combat_action(
        &mut self,
        e: &crate::actions::Action,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        ready: bool,
        now: dereth_primitives::LocalTime,
    ) -> bool {
        // The release table: the same six actions, and only those six.
        if !e.is_start() {
            if !matches!(
                e.id,
                action::COMBAT_LOW_ATTACK
                    | action::COMBAT_MEDIUM_ATTACK
                    | action::COMBAT_HIGH_ATTACK
                    | action::COMBAT_AIM_LOW
                    | action::COMBAT_AIM_MEDIUM
                    | action::COMBAT_AIM_HIGH
            ) {
                return false;
            }
            // End the attack request at the requested attack height with power `-1.0`. `-1.0` is
            // `None` here: attack-request completion consults the bar and the cap only when the
            // power is `-1.0`.
            let height = game.combat.requested_attack_height;
            let before = req.0.len();
            game.end_attack_request(req, height, None, ready, now);
            if req.0.len() > before {
                self.stats.attacks_released += 1;
            }
            return true;
        }
        match e.id {
            // The gauge. `CombatIncreaseAttackPower` and `CombatIncreaseMissileAccuracy` are the
            // increase arm (action ids `0x1000005C` / `0x100000F0`); the
            // other two step down. The desired-attack-power-changed notice is the
            // window's read-back and is applied by the frame.
            a @ (action::COMBAT_DECREASE_ATTACK_POWER
            | action::COMBAT_INCREASE_ATTACK_POWER
            | action::COMBAT_DECREASE_MISSILE_ACCURACY
            | action::COMBAT_INCREASE_MISSILE_ACCURACY) => {
                let increase = a == action::COMBAT_INCREASE_ATTACK_POWER
                    || a == action::COMBAT_INCREASE_MISSILE_ACCURACY;
                game.combat.adjust_ui_requested_power(increase);
                self.stats.desired_power_changes += 1;
                true
            }
            a @ (action::COMBAT_LOW_ATTACK
            | action::COMBAT_MEDIUM_ATTACK
            | action::COMBAT_HIGH_ATTACK
            | action::COMBAT_AIM_LOW
            | action::COMBAT_AIM_MEDIUM
            | action::COMBAT_AIM_HIGH) => {
                let height = match a {
                    action::COMBAT_LOW_ATTACK | action::COMBAT_AIM_LOW => AttackHeight::Low,
                    action::COMBAT_HIGH_ATTACK | action::COMBAT_AIM_HIGH => AttackHeight::High,
                    _ => AttackHeight::Medium,
                };
                // `set_requested_attack_height`, whose tail is `start_attack_request`.
                // The requested height lives in `dereth_client_model`'s own
                // `CombatState`, because the attack-done handler re-fires the
                // auto-repeat swing with it.
                match game.set_requested_attack_height(height, ready, now) {
                    Ok(()) => self.stats.attack_height_changes += 1,
                    Err(text) => {
                        self.refuse(game, text);
                        self.stats.requests_refused += 1;
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Handle the magic-combat keys, including the eighteen bound by default.
    ///
    /// A recognized press emits one spellcasting notice and returns true. Every
    /// release and unknown action returns false:
    ///
    /// | action | notice role |
    /// |---|---|
    /// | `0x10000060` `CombatCastCurrentSpell` | cast the current spell |
    /// | `0x10000061` / `0x10000062` | previous / next spell selection |
    /// | `0x10000063` / `0x10000064` | previous / next spell tab |
    /// | `0x10000065`…`0x10000070` | cast quickslot `action + 0xEFFFFF9B` |
    /// | `0x10000102` / `0x10000103` | first / last spell selection |
    /// | `0x10000104` / `0x10000105` | first / last spell tab |
    ///
    /// **The quickslot range is twelve wide and the shipped keymap binds nine of it.**
    /// The slot is `action - 0x10000065`, so `UseSpellSlot_1` is slot **0**; the cases run to
    /// `0x10000070`, i.e. slot 11, while `MagicCombat`'s defaults stop at `UseSpellSlot_9`. The
    /// three unbound ones are transcribed anyway because a player can bind them.
    ///
    /// The notice is emitted here through `dereth_client_contract::notices::NoticeInbox` and
    /// consumed by `SpellcastingPanel` during `RemainingPanels::update`. This queue
    /// crosses the input/UI boundary; producer and consumer counts remain separate
    /// so emitting a notice cannot masquerade as applying it.
    fn handle_magic_action(&mut self, e: &crate::actions::Action) -> bool {
        use dereth_client_contract::view::MagicNotice as N;
        if !e.is_start() {
            return false;
        }
        let n = match e.id {
            action::COMBAT_CAST_CURRENT_SPELL => N::CastCurrentSpell,
            action::COMBAT_PREV_SPELL => N::PrevSpellSelection,
            action::COMBAT_NEXT_SPELL => N::NextSpellSelection,
            action::COMBAT_PREV_SPELL_TAB => N::PrevSpellTab,
            action::COMBAT_NEXT_SPELL_TAB => N::NextSpellTab,
            action::COMBAT_FIRST_SPELL => N::FirstSpellSelection,
            action::COMBAT_LAST_SPELL => N::LastSpellSelection,
            action::COMBAT_FIRST_SPELL_TAB => N::FirstSpellTab,
            action::COMBAT_LAST_SPELL_TAB => N::LastSpellTab,
            a if (action::USE_SPELL_SLOT_FIRST..=action::USE_SPELL_SLOT_LAST).contains(&a) => {
                // The slot is `action - 0x10000065`.
                N::CastQuickslotSpell {
                    slot: (a.0 - action::USE_SPELL_SLOT_FIRST.0) as usize,
                }
            }
            _ => return false,
        };
        self.magic_notices.emit(n);
        self.stats.magic_actions += 1;
        true
    }

    /// **Local chat-command dispatch, used by the methods below.**
    ///
    /// `on_chat_command` resolves `@tell`/`@t`/`@send`/`@whisper`/`@w`,
    /// `@reply`/`@r`/`@rp` and `@retell`/`@rt` to their handler names — including the
    /// trailing-comma trim that turns `@tell bob,` into the verb `tell`, which is exactly the
    /// shape players type. This is the dispatch for that answer.
    ///
    /// **Why the three handlers are in `dereth_client_model::chat` and only this table is here.**
    /// They read and write `ChatState::last_teller` and `last_tellee_name`; the decision "which
    /// player does
    /// this line go to" is the model's, and this file's job is to turn the answer into a
    /// [`Request`] and a notice. This module wires.
    ///
    /// The refusals are the client's own four strings on chat type `0x1A`, raised as
    /// [`Notice::DisplayString`] because that is what the handler's scroll write of type `0x1A`
    /// to the current command source's window is.
    ///
    /// [`Interaction::absorb`] passes each `DisplayString` to
    /// [`dereth_client_model::scroll::Scroll`] for fan-out. Type `0x1A` appears
    /// in the strip across the viewport top because the main chat window's default
    /// filter clears bit 26.
    ///
    /// The window id is still lost on the way: `Notice::DisplayString` carries a channel and no
    /// command source, so the fan-out sends window `0` — a broadcast — where the
    /// client would have sent the id of the chat window the command was typed into. That is
    /// unobservable for `0x1A` (no window's default filter accepts it, so the id decides nothing)
    /// and would matter for a command that answered on a filtered-in type.
    /// The motion commands this frame's poses issued, for
    /// [`crate::app::App::apply_input_actions`] to hand to `MovementCommands`.
    pub fn take_pose_motions(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_pose_motions)
    }
}
