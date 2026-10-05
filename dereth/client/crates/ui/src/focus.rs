//! Focus, activation, input-action routing, mouse capture, hit testing and drag-and-drop.
//!
//! Two rules here decide whether the interface feels like AC's:
//!
//! * **The hit test honours a per-pixel alpha mask.** The point test is
//!   an inclusive box test *plus* a test against the element's alpha image. That is
//!   how the irregular panel frames let clicks through their transparent corners. The search is
//!   depth-first, **tail → head** (top-most first), with coordinates made parent-relative on the
//!   way down, and an element that is neither mouse-visible nor click-blocking is transparent to
//!   the mouse *but its children are still searched*.
//! * **Keyboard focus and mouse capture are independent.** The focus element receives keys;
//!   the element with mouse capture receives motion and release wherever the pointer is.

use dereth_primitives::LocalTime;

use crate::msg::element::id as msgid;
use crate::msg::MessagePoint;
use crate::{ElemHandle, UiSystem};

/// One input action delivered by the input manager.
///
/// `dereth-input` owns the action catalogue and the maps; this is the shape the UI receives. `start` is
/// the press edge; UI action handling acts only on that edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub action: u32,
    pub start: bool,
    pub x: i32,
    pub y: i32,
}

/// The input action ids the UI itself handles.
///
/// The catalogue itself is [`dereth_input::action`]. This
/// re-export keeps `dereth_ui::focus::action::*` resolving for this crate and its callers.
pub use dereth_input::action;

/// The manager's mouse group.
#[derive(Debug, Default, Clone)]
pub struct MouseState {
    /// The element the mouse was last over.
    pub last_over: Option<ElemHandle>,
    /// The element the mouse last entered.
    pub last_entered: Option<ElemHandle>,
    /// The element with mouse capture.
    pub capture: Option<ElemHandle>,
    /// The actions triggering capture — capture is released when this set empties.
    pub actions_triggering_capture: Vec<u32>,
    /// The mouse-capture reference count.
    pub capture_count: i32,
    /// Whether to perform the mouse hit test.
    pub perform_hit_test: bool,
    /// The mouse has left the window — suppresses all hit testing until the next move inside.
    pub has_left_window: bool,
    pub pos: (i32, i32),
    /// Which element each held action was pressed on, for the 0x19 "click" rule.
    pub pressed_on: Vec<(u32, ElemHandle)>,
}

/// The manager's drag group.
#[derive(Debug, Default, Clone)]
pub struct DragState {
    /// The element that may become a drag source after the movement threshold.
    pub potential: Option<ElemHandle>,
    /// The drag proxy.
    pub element: Option<ElemHandle>,
    /// The element that owns the drag.
    pub owner: Option<ElemHandle>,
    /// Whether drag processing has started.
    pub started: bool,
    /// Where the press happened.
    pub origin: (i32, i32),
    /// The last drop-catching element under the cursor.
    pub last_drag_cursor_over: Option<ElemHandle>,
    /// Where inside the proxy the cursor holds it — the `(x, y)` offset pair
    /// the drag start is given and then subtracts:
    /// the proxy moves to `(mouse x - grab x, mouse y - grab y)`.
    ///
    /// The generic path passes the cursor's offset inside the element being dragged, so a window
    /// picked up by its title bar does not jump. The item list's begin-drag
    /// passes the constant `(0x10, 0x10)` instead, which centres a 32×32 item icon on
    /// the pointer whatever part of the slot was pressed.
    pub grab: (i32, i32),
}

/// The drag-and-drop record (reference-counted in the original).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragDropInfo {
    pub dragged: ElemHandle,
    pub owner: ElemHandle,
    pub target: Option<ElemHandle>,
    pub success: bool,
}

/// Starting a drag requires `dx² + dy² > 15` — about four pixels.
///
/// The constant lives in [`dereth_input::action`] with the action ids; this re-export
/// keeps `dereth_ui::focus::DRAG_THRESHOLD_SQUARED` resolving.
pub use dereth_input::action::DRAG_THRESHOLD_SQUARED;

/// Left, right and middle **double**-click, in the order the click fold maps them
/// onto 7, 8 and 9.
///
/// In the shipped bindings, input map 3 binds
/// `DIMOFS_BUTTON0` twice, once with activation `Click` (0x03) to action 7 and once with
/// `MouseDblClick` (0x60) to action 0x0A. The binding matcher prefers the larger
/// activation, so the second press of a pair resolves to 0x0A and **not** to 7.
pub const DOUBLE_CLICK_ACTIONS: [u32; 3] = [0x0A, 0x0B, 0x0C];

impl UiSystem {
    // ---- hit testing ---------------------------------------------------------------------------

    /// The mouse hit tester's recursion.
    ///
    /// Invisible elements refuse immediately. Children are tested from the list tail toward the
    /// head, with coordinates rebased into each child. If no child accepts, the element itself
    /// accepts only when it is mouse-visible or blocks clicks.
    #[must_use]
    pub fn hit_test(&self, at: ElemHandle, x: i32, y: i32) -> Option<ElemHandle> {
        let n = self.node(at)?;
        if !n.region.flags.visible {
            return None;
        }
        // tail -> head: top-most first.
        for c in n.region.children.iter().rev() {
            let Some(cn) = self.node(*c) else { continue };
            if cn.region.point_is_over(x, y) {
                let (dx, dy) = (cn.region.box_.x0, cn.region.box_.y0);
                if let Some(found) = self.hit_test(*c, x - dx, y - dy) {
                    return Some(found);
                }
            }
        }
        if n.is_mouse_visible || n.region.flags.block_clicks {
            return Some(at);
        }
        None
    }

    /// Hit-test the whole tree from the root, in screen coordinates.
    #[must_use]
    pub fn hit_test_screen(&self, x: i32, y: i32) -> Option<ElemHandle> {
        let root = self.root();
        let rb = self.node(root)?.region.box_;
        if !self.node(root)?.region.point_is_over(x, y) {
            return None;
        }
        self.hit_test(root, x - rb.x0, y - rb.y0)
    }

    // ---- mouse ----------------------------------------------------------------------------------

    /// The manager's mouse-move handler.
    pub fn mouse_move(&mut self, now: LocalTime, x: i32, y: i32) {
        self.tooltip.last_mouse_move_time = now.0;
        self.mouse.pos = (x, y);
        self.mouse.has_left_window = false;
        let hit = self.hit_test_screen(x, y);
        self.switch_mouse_over(hit);

        // A pending drag starts once the threshold is passed.
        if !self.drag.started && self.drag.potential.is_some() {
            let dx = x - self.drag.origin.0;
            let dy = y - self.drag.origin.1;
            if dx * dx + dy * dy > DRAG_THRESHOLD_SQUARED {
                self.start_drag_and_drop();
            }
        }
        // **The mouse-over switch's drag tail** — the only producer of `0x3E`.
        //
        // ```text
        // catcher = none
        // if there is a hit, dragging has started, and a proxy exists:
        //     catcher = the hit's drop-catching ancestor for that proxy
        // if catcher changed:
        //     if an old catcher exists, notify it with message 0x3E, leaving
        //     if a new catcher exists, notify it with message 0x3E, entering
        //     remember the new catcher
        // end if
        // ```
        //
        // The field holds the **drop catcher**, not the raw hit, and without this tail no element
        // would ever be told a drag was over it.
        //
        // Remembering the catcher, rather than the raw hit, is load-bearing downstream:
        // the drag stop uses it as the drop target **directly**, with no catcher
        // lookup of its own: it records it as the target and asks it to catch the dropped item.
        //
        // Placed here rather than in [`Self::switch_mouse_over`] because that helper returns early
        // when the hit has not changed, while the client's tail runs unconditionally; the drag
        // proxy moves under the pointer on every move, so the catcher can change with the hit
        // unchanged only if the tree moved, and `mouse_move` is the one caller a drag can be live
        // under. The other `switch_mouse_over` call sites are all "the pointer left" (`None`), so
        // moving the tail here changes nothing observable.
        if self.drag.started {
            let over = if self.drag.element.is_some() {
                hit.and_then(|h| self.drag_and_drop_catcher(h))
            } else {
                None
            };
            if over != self.drag.last_drag_cursor_over {
                if let Some(old) = self.drag.last_drag_cursor_over {
                    self.broadcast_element_message(old, msgid::DRAG_CURSOR_OVER, 0, 0);
                }
                if let Some(new) = over {
                    self.broadcast_element_message(new, msgid::DRAG_CURSOR_OVER, 1, 0);
                }
                self.drag.last_drag_cursor_over = over;
            }
            if let Some(proxy) = self.drag.element {
                let (gx, gy) = self.drag.grab;
                self.move_to(proxy, x - gx, y - gy);
            }
        }

        // Mouse movement goes to the capture holder when one exists. Otherwise it goes to the
        // last-entered element unless both drag-started is set and a drag proxy exists.
        //
        // **Do not return as soon as drag-started is set.** Drag-started is set before the drag
        // start walks for a draggable ancestor — so it is true after any four-pixel press-and-move, on
        // any element, whether or not anything accepted the drag. An early return would stop the
        // capture holder hearing 0x1E the moment the pointer moved far enough to matter, which is every
        // gesture that is a *drag of the element itself*: a scrollbar thumb
        // (its `0x1E` arm is the only path
        // to its pixel scroll and scroll-to-point), a dragbar, a resizebar and a text
        // selection sweep. Only the **fallback** to the last-entered element is suppressed by a
        // live drag proxy, and only when there really is one.
        let to = self.mouse.capture.or_else(|| {
            if self.drag.started && self.drag.element.is_some() {
                None
            } else {
                self.mouse.last_entered
            }
        });
        if let Some(t) = to {
            let notify = self.node(t).is_some_and(|n| {
                n.flags.notify_on_mouse_move() || !self.mouse.pressed_on.is_empty()
            });
            if notify {
                self.broadcast_element_message_at(
                    t,
                    msgid::MOUSE_MOVE,
                    0,
                    0,
                    self.message_point(t, x, y),
                );
            }
        }
    }

    /// Behavior: suppresses all hit testing until the next move inside.
    pub fn mouse_leave(&mut self) {
        self.mouse.has_left_window = true;
        self.switch_mouse_over(None);
    }

    /// Switch which element the mouse is over.
    pub fn switch_mouse_over(&mut self, to: Option<ElemHandle>) {
        if self.mouse.last_entered == to {
            return;
        }
        if let Some(old) = self.mouse.last_entered {
            if self.is_alive(old) {
                if let Some(n) = self.node_mut(old) {
                    n.region.flags.mouse_over = false;
                    n.region.flags.mouse_over_top = false;
                }
                self.broadcast_element_message(old, msgid::MOUSE_OVER, 0, 0);
                self.broadcast_element_message(old, msgid::MOUSE_OVER_TOP, 0, 0);
            }
        }
        self.mouse.last_entered = to;
        self.mouse.last_over = to;
        if let Some(new) = to {
            if let Some(n) = self.node_mut(new) {
                n.region.flags.mouse_over = true;
                n.region.flags.mouse_over_top = true;
            }
            self.broadcast_element_message(new, msgid::MOUSE_OVER, 1, 0);
            self.broadcast_element_message(new, msgid::MOUSE_OVER_TOP, 1, 0);
        }
        self.stop_hover();
        self.check_cursor();
    }

    /// The last-entered element — the element the pointer is over, as the last hit test left it.
    ///
    /// "The element the mouse is over" in spirit; the client reads the field directly. It is the whole
    /// observable result of the hit test, so it is what a test asserts on.
    #[must_use]
    pub fn mouse_over(&self) -> Option<ElemHandle> {
        self.mouse.last_entered
    }

    /// Behavior: the virtual gate `check_cursor` calls
    /// through before it will take an element's cursor.
    ///
    /// The client's body is "the cursor data id is valid", which is exactly `cursor.is_some()`
    /// here. It is a **virtual** call, so a subclass could refuse to supply one even with the
    /// field set; no shipped element overrides it.
    #[must_use]
    pub fn has_cursor(&self, h: ElemHandle) -> bool {
        self.node(h).is_some_and(|n| n.cursor.is_some())
    }

    /// Behavior: who supplies the cursor right now.
    ///
    /// It does not walk *parents* from the last-entered element, and it does not ignore mouse
    /// capture. The client does:
    ///
    /// ```text
    /// e = the element with mouse capture
    /// if there is none, or it has no cursor:  e = the last-entered element
    /// if e has a cursor:  set_cursor(e's cursor and hot spot, default = false)
    /// else:               set_cursor(the default cursor and hot spot, default = true)
    /// ```
    ///
    /// Two things in that are load-bearing. **Capture outranks hover**, so an element being
    /// dragged keeps its cursor while the pointer is over something else — which is the whole
    /// point of a drag cursor. And the element arm passes **`false`** for `default`, so an
    /// override never overwrites the default cursor: when the pointer leaves, the default that
    /// the client cursor update last pushed is still there to come back to.
    pub fn check_cursor(&mut self) {
        let from = self
            .mouse
            .capture
            .filter(|&h| self.has_cursor(h))
            .or_else(|| self.mouse.last_entered.filter(|&h| self.has_cursor(h)));
        if let Some((did, x, y)) = from.and_then(|h| self.node(h).and_then(|n| n.cursor)) {
            self.set_cursor(did, x, y, false);
            return;
        }
        if let Some((did, x, y)) = self.default_cursor {
            self.set_cursor(did, x, y, true);
        }
    }

    /// The manager's set-cursor.
    ///
    /// A default update records the image and hotspot. A valid image is sent to the device only
    /// when either the image or hotspot differs from the last successful update.
    ///
    /// Two things matter here: the last-cursor cache — a
    /// redundant push must not reach the device, because rebuilding an `HCURSOR` from a dat
    /// surface every frame is not free — and a record of the push that *did* get through, for
    /// the host to act on ([`UiSystem::take_pending_cursor`]). Note the equality test covers the
    /// **hotspot as well as the did**: two states can share a cursor image and differ only in
    /// where its point is, and `UICURSOR` ships four such pairs.
    ///
    /// An invalid data id (a zero [`dereth_primitives::DataId`]) is recorded as the
    /// default when asked and pushed to nothing, exactly as the client does.
    pub fn set_cursor(
        &mut self,
        did: dereth_primitives::DataId,
        hot_x: i32,
        hot_y: i32,
        default: bool,
    ) {
        if default {
            self.default_cursor = Some((did, hot_x, hot_y));
        }
        if did.0 == 0 {
            return;
        }
        let next = Some((did, hot_x, hot_y));
        if self.last_cursor == next {
            return;
        }
        self.last_cursor = next;
        self.pending_cursor = next;
    }

    /// Store the override and hotspot on the element, then choose the active cursor again.
    pub fn element_set_cursor(
        &mut self,
        h: ElemHandle,
        did: dereth_primitives::DataId,
        hot_x: i32,
        hot_y: i32,
    ) {
        if let Some(n) = self.node_mut(h) {
            n.cursor = Some((did, hot_x, hot_y));
        }
        self.check_cursor();
    }

    /// Behavior: the cursor data id is set invalid, hotspots zeroed,
    /// then `check_cursor`.
    pub fn element_unset_cursor(&mut self, h: ElemHandle) {
        if let Some(n) = self.node_mut(h) {
            n.cursor = None;
        }
        self.check_cursor();
    }

    fn message_point(&self, h: ElemHandle, x: i32, y: i32) -> MessagePoint {
        let (ox, oy) = self.screen_origin(h);
        MessagePoint {
            window: (x, y),
            element: (x - ox, y - oy),
        }
    }

    /// The manager's mouse-down event, the six documented steps.
    pub fn mouse_down(&mut self, action: u32, x: i32, y: i32) {
        self.mouse.pos = (x, y);
        let hit = self.hit_test_screen(x, y);
        self.switch_mouse_over(hit);
        let tap = action::is_tap(action);

        // 3. capture
        if !tap {
            if let Some(e) = hit {
                if self.mouse.capture.is_none() {
                    self.set_mouse_capture(e);
                }
                if !self.mouse.actions_triggering_capture.contains(&action) {
                    self.mouse.actions_triggering_capture.push(action);
                }
                if action == action::PRIMARY_CLICK {
                    self.drag.potential = Some(e);
                    self.drag.origin = (x, y);
                }
            }
        }

        // 4. activation
        if let Some(e) = hit {
            if let Some(r) = self.root_of(e) {
                if self.active_element != Some(r) {
                    if self.node(r).is_some_and(|n| n.flags.activatable()) {
                        self.activate(r);
                    } else if let Some(cur) = self.active_element {
                        self.deactivate(cur);
                    }
                }
            }
        }

        // 5. delivery
        if let Some(e) = hit {
            let pt = self.message_point(e, x, y);
            if tap {
                self.broadcast_element_message_at(e, msgid::MOUSE_TAP, action, 0, pt);
            } else {
                self.mouse.pressed_on.push((action, e));
                self.broadcast_element_message_at(e, msgid::MOUSE_PRESS, action, 0, pt);
                // The element's mouse-down: a context-menu element on action 8.
                if action == action::SECONDARY_CLICK
                    && self.node(e).is_some_and(|n| n.flags.context_menu())
                {
                    self.broadcast_element_message_at(e, msgid::CONTEXT_MENU, 0, 0, pt);
                }
            }
        } else {
            // 6. nothing hit at all: drop focus.
            if let Some(f) = self.focus_element {
                self.relinquish_focus(f);
            }
        }

        // 7. The client's tail, which is the
        //    only thing in the whole client that ever takes focus from a press:
        //
        //    ```text
        //    run the base mouse-down handler (step 5 above broadcasts 0x1C)
        //    root = the element's root
        //    if the root exists and is activatable: activate the root, then take focus
        //    ```
        //
        //    It runs **after** the 0x1C broadcast, which is why it is here and not folded into
        //    step 5: the text element asks whether it has focus *before* the
        //    chain and decides between "select the whole box" and "put the caret where the
        //    pointer is" on that answer, so the element's own arm has to see the **old** focus.
        //
        //    **It covers the whole scrollable-element subtree**, which is what
        //    retail does: button element -> text element -> scrollable element, so a click
        //    on any button takes focus off the chat box. Narrowing it to an editable or
        //    selectable text element would be wrong — a list-box element that can never become
        //    the focus element can never register input map `0x0A` from its focus.
        //
        //    There are two visible answers rather than one:
        //
        //    * **A button does not change state.** The default element-message handler's 0x2F
        //      arm pushes state 4 only from 0, 1 or 5, and step 5 above has already put the button
        //      in its own state 3 (`widgets::button::state::PRESSED`) through the 0x1C broadcast.
        //      Retail reaches the same place by the opposite route: the button's mouse-down calls
        //      its base (and so takes focus) first,
        //      so the arm does fire there and the button's state update overwrites it one statement
        //      later.
        //    * **A list box or a plain label does change state**, 0/1 -> 4, and falls back to
        //      state 0 when the layout does not declare 4 (no state description for 4 means
        //      state 0). That is retail's own behaviour.
        //
        //    See [`crate::element::Element::takes_focus_on_press`] for the subtree and for the two
        //    game types it cannot reach.
        //
        //    **It is narrowed away from the wheel, and the wheel only.** In retail a wheel
        //    detent *cannot* reach here: the client's handling of actions 5 and 6
        //    is dead code in the shipped build, because `DIMOFS_WHEEL` is bound only in
        //    `ScrollableControls` (map `0x0A`), whose callback is the focused scrollable itself, so
        //    the walk ends in the client's **key-press**
        //    arm and never in the mouse-down one. Actions 5 and 6 arrive here purely because
        //    the shell routes them through the element mouse-down path — a **declared deviation**
        //    taken to reach element message
        //    `0x1C`, whose only producer is this wheel dispatch and whose stated observable is
        //    "one detent scrolls the scrollable under the pointer by one scroll-delta step".
        //    Moving the keyboard focus is not part of that observable and retail's wheel cannot do
        //    it, so the deviation must not carry this line.
        //
        //    It is not merely cosmetic. Wheeling over the chat log would move the focus from the
        //    entry to the log; the log is selectable and not editable, so `UiShell::wants_text_mode`
        //    would go false and map `0x0A` would be unregistered — and **the second detent would
        //    resolve to no action at all**.
        //
        //    The *other* half — that a focused non-editable scrollable should register `0x0A` at
        //    all, which is the client's unconditional
        //    push — is still open, and is named on
        //    [`Self::takes_focus_on_press`] below. This line does not fix it and does not hide it:
        //    a **click** on the log still takes its focus, which is retail, and
        //    still drops map `0x0A`, which is the open defect.
        if let Some(e) = hit {
            let wheel = action == action::WHEEL_UP || action == action::WHEEL_DOWN;
            if !tap && !wheel && self.takes_focus_on_press(e) {
                let root_ok = self
                    .root_of(e)
                    .is_some_and(|r| self.node(r).is_some_and(|n| n.flags.activatable()));
                if root_ok {
                    self.take_focus(e);
                }
            }
        }
    }

    /// Does the element under the press want to become the focus element? See
    /// [`crate::element::Element::takes_focus_on_press`].
    ///
    /// `pub` because two things outside this function need the answer: the
    /// census over the shipped screens, and focus-driven input-map
    /// registration, which has to key on *"is the focused element scrollable"* and
    /// this is that question. It answers `false` for a handle that is not in the arena and for one
    /// whose behaviour is currently lifted out of its slot: a widget cannot be read back out
    /// of the arena from inside its own handler.
    #[must_use]
    pub fn takes_focus_on_press(&self, h: ElemHandle) -> bool {
        self.node(h)
            .and_then(|n| n.behaviour.as_deref())
            .is_some_and(crate::element::Element::takes_focus_on_press)
    }

    /// Which input actions are currently held on `h`, corresponding to the element's
    /// mouse-down table.
    ///
    /// The button click handler opens by asking exactly this of the message's own element ("is
    /// action 7 or action 10 in its mouse-down table?") before it
    /// will treat a move as a thumb drag.
    #[must_use]
    pub fn is_pressed_on(&self, h: ElemHandle, action: u32) -> bool {
        self.mouse
            .pressed_on
            .iter()
            .any(|(a, e)| *a == action && *e == h)
    }

    /// The manager's mouse-up event → the element's mouse-up.
    ///
    /// **Checked against retail's element mouse-up.**
    /// The element first snapshots whether this action was pressed on it, runs the base mouse-up
    /// behavior, and always broadcasts release `0x1D`. It stops if the pointer has left or the
    /// press began elsewhere. Actions 10/11/12 fold to 7/8/9 for a normal click; an element that
    /// wants double clicks instead receives `0x1A` with the original action. Every other accepted
    /// release broadcasts click `0x19` with its action.
    ///
    /// Three things here are each observable:
    ///
    /// * **the mouse-over-top gate**: without it a press that dragged off a button and
    ///   released elsewhere would still fire the button's click. That is the behaviour a person notices
    ///   first, and it is why the gate is line (1) of the original rather than an afterthought;
    /// * **0x1A and 0x19 are exclusive.** Raising both on a double-clicked row would raise the
    ///   click as well as the activation, so the character-management screen would re-select on the very
    ///   message that was supposed to enter the world;
    /// * **the 10/11/12 → 7/8/9 fold**: without it an element that does *not* want double
    ///   clicks would see `p1 = 10` on the second click of a pair where the client gives it `p1 = 7`.
    ///   The button's mouse-up tests `action == 7 || action == 10` precisely
    ///   because both spellings reach it.
    ///
    /// `double_click` is kept for a caller that has no 10/11/12 action of its own to speak with —
    /// the discriminator in the client is the **action id**, and the real path supplies it.
    pub fn mouse_up(&mut self, action: u32, x: i32, y: i32, double_click: bool) {
        self.mouse.pos = (x, y);
        self.mouse
            .actions_triggering_capture
            .retain(|a| *a != action);
        let target = self.mouse.capture.or_else(|| self.hit_test_screen(x, y));
        if self.mouse.actions_triggering_capture.is_empty() {
            self.release_mouse_capture();
        }
        if self.drag.started {
            // The drag cleanup suppresses the ordinary release **only when there is a
            // proxy**:
            //
            // ```text
            // if drag processing started:
            //     if a drag proxy exists:               // <- the gate is the proxy, not the started flag
            //         suppress the ordinary mouse-up target
            //         release capture and restore mouse-over if capture exists
            //         notify the drag's owning element of mouse-up action 7 if there is one
            //     end if
            //     stop drag-and-drop
            // end if
            // clear the potential drag source
            // ```
            //
            // `started` is set before the draggable walk, as the client does,
            // so the *rejected* drag — a press that moved off a button with nothing draggable
            // anywhere up the chain — reaches here with the flag set and no proxy. Gating the
            // suppression on the flag would swallow that button's `0x1D`, and the missing release
            // would leave the button's pressed flag set: the
            // button would stay in state 2 instead of returning to 1. The client's own gate is the
            // proxy, and it is why retail buttons survive the same sequence.
            let had_proxy = self.drag.element.is_some();
            self.stop_drag_and_drop();
            self.drag.potential = None;
            if had_proxy {
                self.mouse.pressed_on.retain(|(a, _)| *a != action);
                return;
            }
        }
        self.drag.potential = None;
        let pressed = self
            .mouse
            .pressed_on
            .iter()
            .position(|(a, _)| *a == action)
            .map(|i| self.mouse.pressed_on.remove(i).1);
        let Some(e) = target else { return };
        let pt = self.message_point(e, x, y);
        // The element snapshots whether this action existed in its base mouse-down table before
        // chaining to the base mouse-up behavior. The Rust manager owns that
        // table and has already removed the entry above, so p2 carries the authoritative snapshot
        // only across this synchronous internal release broadcast.
        let was_pressed_here = u32::from(pressed == Some(e));
        self.broadcast_element_message_at(e, msgid::MOUSE_RELEASE, action, was_pressed_here, pt);
        // (1) not mouse-over-top: the press dragged off, so no click.
        if !self.node(e).is_some_and(|n| n.region.flags.mouse_over_top) {
            return;
        }
        // (2) no mouse-down table entry: the press was on someone else.
        if pressed != Some(e) {
            return;
        }
        // (3) the double-click actions fold onto their single-click spellings.
        let folded = match DOUBLE_CLICK_ACTIONS.iter().position(|a| *a == action) {
            Some(0) => action::PRIMARY_CLICK,
            Some(1) => action::SECONDARY_CLICK,
            Some(_) => action::MIDDLE_CLICK,
            None => action,
        };
        let is_double = folded != action || double_click;
        if is_double && self.node(e).is_some_and(|n| n.flags.wants_dbl_clicks()) {
            // `p1` stays the *double-click* action: the client never rewrites it on this branch.
            self.broadcast_element_message_at(e, msgid::MOUSE_DOUBLE_CLICK, action, 0, pt);
        } else {
            self.broadcast_element_message_at(e, msgid::MOUSE_CLICK, folded, 0, pt);
        }
    }

    /// The pointer is taken from this interface with its buttons possibly still down: another
    /// interface is shown, and the releases will reach that one. Every held press ends here as a
    /// press dragged off its element ends — the element hears its release and no click — a drag
    /// in flight drops on nothing, and the capture goes, so this interface's next press starts
    /// clean whenever it is shown again.
    pub fn release_pointer(&mut self) {
        self.switch_mouse_over(None);
        self.drag.last_drag_cursor_over = None;
        let mut held: Vec<u32> = Vec::new();
        for a in self
            .mouse
            .actions_triggering_capture
            .iter()
            .chain(self.mouse.pressed_on.iter().map(|(a, _)| a))
        {
            if !held.contains(a) {
                held.push(*a);
            }
        }
        let (x, y) = self.mouse.pos;
        for action in held {
            self.mouse_up(action, x, y, false);
        }
        self.stop_drag_and_drop();
        self.drag.potential = None;
        self.mouse.pressed_on.clear();
        self.mouse.actions_triggering_capture.clear();
        self.mouse.capture = None;
        self.mouse.capture_count = 0;
    }

    /// The input-device manager's mouse x and y — where the pointer is, in screen coordinates.
    ///
    /// The client reads these off the input-device manager singleton from anywhere; here the
    /// manager's own mouse position is the same value and this is the accessor for it. The radar
    /// uses it: it hit-tests the blip field against
    /// the raw cursor on every redraw rather than waiting to be told about a mouse move.
    #[must_use]
    pub fn mouse_pos(&self) -> (i32, i32) {
        self.mouse.pos
    }

    /// Take the mouse capture. Reference-counted.
    pub fn set_mouse_capture(&mut self, h: ElemHandle) {
        self.mouse.capture = Some(h);
        self.mouse.capture_count += 1;
    }

    /// Behavior: the count must reach zero before capture actually drops.
    pub fn release_mouse_capture(&mut self) {
        self.mouse.capture_count -= 1;
        if self.mouse.capture_count <= 0 {
            self.mouse.capture_count = 0;
            self.mouse.capture = None;
        }
    }

    /// The mouse update — step 3 of `use_time`, run only when `perform_hit_test` is set.
    pub fn do_mouse_update(&mut self, now: LocalTime) {
        if self.mouse.has_left_window {
            return;
        }
        let (x, y) = self.mouse.pos;
        self.tooltip.last_mouse_move_time = now.0;
        let hit = self.hit_test_screen(x, y);
        self.switch_mouse_over(hit);
        self.mouse.perform_hit_test = false;
    }

    // ---- focus and activation ---------------------------------------------------------------

    /// The manager's set-focus-element.
    ///
    /// 1. same element → nothing; 2. broadcast **0x2F** with `p1 = 0` on the old one;
    /// 3. store; 4. broadcast **0x2F** with `p1 = 1` on the new one.
    ///
    /// **There is no fifth step** walking the new focus element's ancestor chain writing each
    /// one's focus descendant. Besides the two
    /// focus-changed broadcasts (and unregistering the old element's input maps), the
    /// set-focus-element does exactly two things — writes the manager's own focus field and, on
    /// the gain arm, registers the new element's input maps at priority 3000 — and the element's
    /// focus descendant is not one of them. Its only
    /// writers are take-focus and relinquish-focus, both of which write it on the **root element
    /// alone** and store the focused element **itself**, not the intermediate child on the path.
    ///
    /// The difference is exactly what [`Self::activate`]'s tail reads, so an ancestor walk would
    /// hand a window's activation the wrong element — its own immediate child rather than
    /// the descendant that had the caret. Nothing else reads the field.
    pub fn set_focus_element(&mut self, h: Option<ElemHandle>) {
        if self.focus_element == h {
            return;
        }
        if let Some(old) = self.focus_element {
            if self.is_alive(old) {
                self.broadcast_element_message(old, msgid::FOCUS_CHANGED, 0, 0);
            }
        }
        self.focus_element = h;
        if let Some(new) = h {
            self.broadcast_element_message(new, msgid::FOCUS_CHANGED, 1, 0);
        }
    }

    /// The element's take-focus.
    ///
    /// It finds the root, sets this element's wants-focus flag, gives the manager this element
    /// only when the root is active, and always records this element as the root's focus
    /// descendant.
    ///
    /// Two things follow, and this build implements one of them.
    ///
    /// * **Implemented: the record is on the root and it is `this`.** A window remembers which
    ///   descendant wanted the caret whether or not it got it, which is what makes
    ///   [`Self::activate`]'s "set the focus element to the focus descendant" tail meaningful.
    /// * **Not implemented, deliberately: the is-active gate on the root.** Retail moves the
    ///   manager's focus only when the window is already active, and returns `false` when it is
    ///   not. This implementation also allows programmatic focus on roots visible from construction,
    ///   including character creation and chat entry. Activate-on-show still runs on visibility edges.
    pub fn take_focus(&mut self, h: ElemHandle) {
        let Some(root) = self.root_of(h) else { return };
        if let Some(n) = self.node_mut(h) {
            n.flags.set_wants_focus(true);
        }
        // The deviation above: retail does this only when the root is active.
        self.set_focus_element(Some(h));
        if let Some(n) = self.node_mut(root) {
            n.focus_descendant = Some(h);
        }
    }

    /// Relinquish focus from an element.
    ///
    /// **The transfer is unconditional**, and the reason matters. It is *not* that
    /// block is never returned — the lose-focus callback would return
    /// block, which is **0**, so block is the only result it could produce.
    /// The decisive fact is that **nothing calls it.** Retail has exactly three
    /// input-action callback holders, and dispatches only their action callback
    /// when the input manager sends an action to listeners; no lose-focus callback is dispatched.
    /// Focus transfer does not consult them
    /// either: it broadcasts element message `0x2F` and calls
    /// the old element's input-map unregistration and
    /// the new element's registration, and that is
    /// all. See [`crate::CallbackLoseFocusResult`] for the whole sweep and its calibration.
    ///
    /// The first two statements are the ones that outlive the call:
    /// clearing the root's focus descendant and clearing the wants-focus flag (`0x400000`), both
    /// **unconditional** — they run even when this element is not the one the manager is holding,
    /// which is how a window forgets a wish that was never granted.
    pub fn relinquish_focus(&mut self, h: ElemHandle) {
        let Some(root) = self.root_of(h) else { return };
        if let Some(n) = self.node_mut(root) {
            n.focus_descendant = None;
        }
        if let Some(n) = self.node_mut(h) {
            n.flags.set_wants_focus(false);
        }
        if self.focus_element == Some(h) {
            self.set_focus_element(None);
        }
    }

    #[must_use]
    pub fn focus_element(&self) -> Option<ElemHandle> {
        self.focus_element
    }

    #[must_use]
    pub fn active_element(&self) -> Option<ElemHandle> {
        self.active_element
    }

    /// The element's activate plus the manager's activation alert.
    ///
    /// Activation does **not** broadcast `0x3E` — the *drag-cursor-over*
    /// message. The activation alert's **activation** arm
    /// broadcasts **nothing at all**: it notifies the input-device manager that the active element
    /// is changing, deactivates the old active element, registers the new one's input maps at 2000
    /// and asks whether it is activatable, and sets attribute `0x33` when it says no. The only
    /// broadcast in the function is `0x2F` on the **deactivation**
    /// arm, and it goes to the focus element rather than to the element being deactivated — see
    /// [`Self::deactivate`].
    ///
    /// Activation refuses invisible
    /// elements, forwards a non-root request to its root, sets the active flag, registers input
    /// maps, sends the manager's activation alert, registers the element as activatable, and
    /// brings it to the front. On the inactive-to-active edge it also broadcasts `0x29` and
    /// restores the root's remembered focus descendant.
    ///
    /// Three parts are easy to miss:
    ///
    /// * **the root-element gate.** Activating anything that is not a root element **forwards
    ///   to its root**, so activating a button activates the window it lives in. Bit 21 is
    ///   is-root-element — the root-element setter is the only writer of `0x200000` — and *not*
    ///   activatable (bit 2, attribute 0x33), which is the manager mouse-down's test one level up;
    /// * **`register_activatable`**; without it an element activated by a click would never be in
    ///   the activatable list and [`Self::activate_next`] could not fall back to it;
    /// * **the focus tail.** A window that remembers which descendant had focus
    ///   (this build's `focus_descendant`) hands it straight back
    ///   on activation. Without it, clicking back onto a window leaves the caret wherever it was.
    ///
    /// The visibility guard pairs with the activate-on-show producer:
    /// direct activation must also refuse hidden descendants and unattached elements.
    ///
    /// The priority-0 registration and the activation alert's priority-2000 registration are **both**
    /// input-manager writes, which `UiSystem` deliberately cannot make. The tree half of them is
    /// [`Self::input_maps_for_registration`]; the mirror is `UiShell::active_input_maps`.
    pub fn activate(&mut self, h: ElemHandle) -> bool {
        if !self.is_visible(h) {
            return false;
        }
        let Some(n) = self.node(h) else { return false };
        let flags0 = n.flags;
        if !flags0.is_root_element() {
            // Not a root element: activate the root instead, or fail if there is none.
            let Some(r) = self.root_of(h) else {
                return false;
            };
            if r == h {
                // The root lookup walks up until the root-element flag; if it answered `h` itself
                // without the flag being set the tree is malformed, and retail would recurse for
                // ever. Refuse instead of hanging.
                return false;
            }
            return self.activate(r);
        }
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_active(true);
        }
        // The activation alert — its own "already the active element" early return, then
        // the old one's deactivate, then the store.
        if self.active_element != Some(h) {
            if let Some(old) = self.active_element {
                self.active_element = Some(h);
                self.deactivate(old);
            }
            self.active_element = Some(h);
            // If the element is not activatable, attribute 0x33 is set true.
            // The newly active element is marked activatable whether the layout said so or not.
            if let Some(n) = self.node_mut(h) {
                n.flags.set_activatable(true);
            }
        }
        self.register_activatable(h);
        self.bring_to_front(h);
        if !flags0.is_active() {
            self.broadcast_element_message(h, msgid::ACTIVATED, 0, 0);
            if let Some(f) = self.node(h).and_then(|n| n.focus_descendant) {
                self.set_focus_element(Some(f));
            }
        }
        true
    }

    /// The element's deactivate plus the manager's activation alert,
    /// deactivation arm.
    ///
    /// The arm is
    ///
    /// ```text
    /// if the element is the active element:
    ///     no active element
    ///     if there is a focus element:
    ///         broadcast 0x2F (0, 0) to it, unregister its input maps, clear the focus element
    /// ```
    ///
    /// — which is exactly [`Self::set_focus_element`]`(None)`'s losing half, so it is expressed as
    /// that call rather than as a second copy. Deactivate's own tail then clears the focus
    /// element again, which is a no-op by then; this build makes the same two
    /// calls collapse the same way.
    ///
    /// The `0x2F` here goes to the **focus** element, not to `h`: no new `0x2F` reaches an element
    /// that would not already have received one when focus was dropped.
    ///
    /// Deactivation forwards a non-root request to its root,
    /// clears the active flag, unregisters input maps, and sends the manager's deactivation
    /// alert. Only an active-to-inactive edge broadcasts `0x2A`; a remembered focus descendant
    /// then causes the manager focus to be cleared.
    ///
    /// The `0x2A` is not unconditional: otherwise an element that was never active
    /// would raise a deactivation. And the tail's focus clear is guarded by
    /// **the focus descendant**, not by the manager's focus — a window with no remembered descendant
    /// does not clear the manager's focus on its way out, which is the asymmetry with `activate`.
    pub fn deactivate(&mut self, h: ElemHandle) -> bool {
        let Some(n) = self.node(h) else { return false };
        let flags0 = n.flags;
        if !flags0.is_root_element() {
            let Some(r) = self.root_of(h) else {
                return false;
            };
            if r == h {
                return false;
            }
            return self.deactivate(r);
        }
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_active(false);
        }
        // The deactivation alert: nothing unless this is the active element, then the store and
        // the focus drop, which is `set_focus_element(None)`'s losing half.
        if self.active_element == Some(h) {
            self.active_element = None;
            if self.focus_element.is_some() {
                self.set_focus_element(None);
            }
        }
        if flags0.is_active() {
            self.broadcast_element_message(h, msgid::DEACTIVATED, 0, 0);
            if self.node(h).and_then(|n| n.focus_descendant).is_some() {
                self.set_focus_element(None);
            }
        }
        true
    }

    /// **The base input-map registration behavior, expressed as a pure tree query.**
    ///
    /// This is the *only* behavior the client's priority-2000 and priority-0
    /// input-map registrations can reach, because every class that is ever a
    /// root element — a panel element, `Dialog` and its five subclasses, and all 60-odd screen classes —
    /// inherits the same base registration behavior. The implementation returns
    /// early without an input manager, asks the parent to register at one lower priority, and
    /// then registers this element's nonzero input map at the requested priority.
    ///
    /// So: **the parent's maps go in first, one priority lower, and each generation
    /// drops another one** — a root element activated at the unfocused-UI priority (2000) puts its
    /// parent's map at 1999 and its grandparent's at 1998 — and then the element's own map goes
    /// in at
    /// `priority`. The answer is in the client's own **registration** order; the walk order is what
    /// [`dereth_input::dispatch::InputMapStack::register`] makes of it.
    ///
    /// **In the shipped data this returns empty for every element that can be activated, and the
    /// mechanism is the census, not a coincidence.** Attribute `0x4E` appears **2 times in 2 162
    /// elements across all 101 layouts** of `client_local_English.dat`, both times as `9`
    /// (`DialogBoxes`) and both times on a **leaf edit field** — one in layout `0x21000016`, one in
    /// the `Dialog` layout `0x2100003C` under root `0x2C`. Neither is a root element, and a root
    /// element's ancestors are the framework roots, which carry no map either. Nor can code supply
    /// one: the only code reference to attribute `0x4E` outside the layout loader belongs to the
    /// control-name mapper's key table, so the input map's only source is the layout property.
    /// The call is still implemented, because the *absence of
    /// the band* and *an empty band* are different things and only the second one is retail.
    #[must_use]
    pub fn input_maps_for_registration(&self, h: ElemHandle, priority: i32) -> Vec<(u32, i32)> {
        let mut out = Vec::new();
        // The recursion is parent-first, so build the chain and walk it from the top down.
        let mut chain = Vec::new();
        let mut cur = Some(h);
        while let Some(c) = cur {
            chain.push(c);
            cur = self.parent(c);
        }
        // `chain[0]` is `h` at `priority`, `chain[1]` its parent at `priority - 1`, and so on; the
        // deepest recursion runs first.
        for (up, c) in chain.iter().enumerate().rev() {
            if let Some(map) = self.node(*c).and_then(|n| n.input_map) {
                if map != 0 {
                    out.push((map, priority - i32::try_from(up).unwrap_or(i32::MAX)));
                }
            }
        }
        out
    }

    /// Register this element as activatable.
    /// The activatable list — the array the client's registration appends to and
    /// [`Self::activate_next`] walks backwards. Read-only, so that
    /// "activate registered the window" is assertable from outside the crate.
    #[must_use]
    pub fn activatable_elements(&self) -> &[ElemHandle] {
        &self.activatable
    }

    pub fn register_activatable(&mut self, h: ElemHandle) {
        if !self.activatable.contains(&h) {
            self.activatable.push(h);
        }
    }

    /// Behavior: **not** tab-cycling. Checked against retail.
    ///
    /// ```text
    /// e = the last element of the activatable list; none -> return
    /// while e is not visible:
    ///     e = the previous element (walk *backwards* from the end)
    ///     stop at the root element or at an empty entry
    /// activate e
    /// ```
    ///
    /// Three things follow, and all three are easy to get the other way round:
    /// the walk runs **from the end of the array towards the front**, never wrapping; it starts
    /// from the last element rather than from the active element; and its `bool` parameter is
    /// **never read** — the call site passes `false`, but the body does not mention it. So there
    /// is no "forward" and no "wrap in both directions".
    ///
    /// **Its one call site says what it is for.** The element's visibility setter
    /// calls it on the branch where an element has just become **invisible**,
    /// immediately after unregistering it as activatable — "the window that owned input was hidden,
    /// so give input to the next visible window down the stack". It is a *close* handler, not a
    /// keyboard gesture, and nothing in the client binds a key to it.
    ///
    /// [`crate::UiSystem::set_visible`]'s active-root hide arm calls this.
    pub fn activate_next(&mut self) {
        let Some(&last) = self.activatable.last() else {
            return;
        };
        let mut i = self.activatable.len() - 1;
        let mut e = last;
        while !self.is_visible(e) {
            if i == 0 {
                return;
            }
            i -= 1;
            e = self.activatable[i];
            if e == self.root() {
                return;
            }
        }
        self.activate(e);
    }

    /// The manager's key-press event, whose whole body is:
    ///
    /// ```text
    /// if the debug console has input: return
    /// f = the focus element, or else the active element
    /// if there is no f, or f does not consume the action: broadcast global message 1 (action)
    /// run the visibility-toggle action dispatch (action)
    /// ```
    ///
    /// The focused element first, then the active one, then — when nobody consumed it — **global
    /// message 1**, and then, **whether or not anybody consumed it**,
    /// [`Self::dispatch_input_action`].
    ///
    /// **That tail must not be skipped.** An early `return true` on a consumed action would skip
    /// it: in the client the call sits after the `if`/`else`, not inside it.
    pub fn key_press(&mut self, e: &InputEvent) -> bool {
        let target = self.focus_element.or(self.active_element);
        let consumed = target.is_some_and(|h| self.dispatch_action(h, e));
        if !consumed {
            self.broadcast_global(crate::msg::global::KEY_DOWN_UNCONSUMED, e.action);
        }
        // The visibility-toggle dispatch — unconditional, and outside the `if`.
        self.dispatch_input_action(e.action);
        consumed
    }

    /// Behavior: the **only** reader of
    /// the element input-action listener table, the table property `0x57` fills.
    ///
    /// ```text
    /// bucket = the table's entry for the action; none -> return false
    /// for each listener in the bucket: broadcast element message 0x31 (action, 0) to it
    /// return true
    /// ```
    ///
    /// It does **not** run each listener's action callback. Retail raises **element message
    /// `0x31`**, and the client's `0x31` arm reads attribute
    /// **`0x58`** — `1` **toggle**, `2` show, `3` hide — which is the leg that actually opens a
    /// panel (see `crate::UiSystem::base_listen_to_element_message`'s `0x31` arm).
    ///
    /// Measured over the shipped dats: of the
    /// 102 layout dats, 101 decode, they hold **2,162** element descriptions, **41** carry `0x57`,
    /// **41** carry `0x58`, and every one of those 41 values is **`1`** — so every one of them is
    /// a panel a key is supposed to *toggle*. `PlainElement::on_action` returns `false`, so running
    /// the listeners' action callbacks could not move any of them.
    ///
    /// The return is the client's: `true` when the action **had a bucket**, false when it had
    /// none — not "somebody consumed it". An empty bucket still answers `true`, which is why
    /// the key-press event ignores the result.
    ///
    /// It fans out to **every** registered listener rather than stopping at the first; the loop
    /// has no break.
    pub fn dispatch_input_action(&mut self, action: u32) -> bool {
        let Some(listeners) = self.input_action_listeners.get(&action).cloned() else {
            return false;
        };
        for h in listeners {
            self.broadcast_element_message(h, msgid::VISIBILITY_TOGGLE, action, 0);
        }
        true
    }

    /// The element's action callback — the base handles nothing itself and calls
    /// its child-action hook so a container can intercept.
    ///
    /// **The bubble is essential.** Calling the focused element's `on_action` and stopping would
    /// let no container anywhere see a descendant's action, and every child-action hook in the
    /// client (the chat interface's, the floaty chat window's, the examination window's and the
    /// floaty examination window's) would be unreachable. There is no serial number and no
    /// nesting here, just the base handler's chain to the parent.
    ///
    /// The order is the client's and it matters: the text element's action callback runs its own
    /// arms only when the base action handler did not consume the event and it is a press, so the
    /// **parent chain is asked first** and the text box's own arms are the fallback. Enter in the
    /// chat entry is the case that proves it: the text element's `0x25` arm merely relinquishes
    /// focus, and the chat interface's child-action arm for `0x25` sends the line.
    pub fn dispatch_action(&mut self, h: ElemHandle, e: &InputEvent) -> bool {
        if self.dispatch_child_action(h, e) {
            return true;
        }
        let Some(mut b) = self.take_behaviour(h) else {
            return false;
        };
        let mut ctx = crate::ElemCtx { ui: self, me: h };
        let r = b.on_action(&mut ctx, e);
        self.put_behaviour(h, b);
        r
    }

    /// Behavior: walk `child`'s ancestors, nearest first, giving each
    /// one [`crate::Element::on_child_action`] until one consumes the event.
    ///
    /// `child` never changes as the walk climbs: the base forwards the original
    /// `(child, event)` arguments to its parent unchanged.
    ///
    /// That detail is load-bearing rather than pedantic. The chat interface's eight
    /// arms all sit behind "the child is the chat entry", and the entry is bound with
    /// a recursive child lookup — it is not necessarily the window's immediate child. Had the walk
    /// re-based the child at each level, the guard would have compared the window's own immediate
    /// child against the entry and never matched.
    pub fn dispatch_child_action(&mut self, child: ElemHandle, e: &InputEvent) -> bool {
        let mut at = child;
        while let Some(p) = self.parent(at) {
            if let Some(mut b) = self.take_behaviour(p) {
                let mut ctx = crate::ElemCtx { ui: self, me: p };
                let consumed = b.on_child_action(&mut ctx, child, e);
                self.put_behaviour(p, b);
                if consumed {
                    return true;
                }
            }
            at = p;
        }
        false
    }

    /// The client's entry point: characters go to the focused
    /// element.
    pub fn character(&mut self, ch: u16) {
        let Some(h) = self.focus_element else { return };
        let Some(mut b) = self.take_behaviour(h) else {
            return;
        };
        let mut ctx = crate::ElemCtx { ui: self, me: h };
        b.character(&mut ctx, ch);
        self.put_behaviour(h, b);
    }

    // ---- drag and drop ------------------------------------------------------------------------

    /// The UI system's drag group — dragged element, owning element, started flag and the
    /// remaining drag state — read-only, for a host or a test that has to see a drag in flight.
    ///
    /// [`Self::is_dragging`] answers one question; this is the whole group, because what matters
    /// is *which* element is being dragged and what it is carrying, not merely that something is.
    #[must_use]
    pub const fn drag_state(&self) -> &DragState {
        &self.drag
    }

    /// Start drag-and-drop the way the manager's mouse-move handler does:
    /// with the potential drag element and the cursor's
    /// offset inside that element.
    ///
    /// ```text
    /// source = potential drag element
    /// offset = cursor position - source screen origin
    /// ```
    pub fn start_drag_and_drop(&mut self) {
        let Some(src) = self.drag.potential else {
            return;
        };
        let (ox, oy) = self.screen_origin(src);
        let (x, y) = self.mouse.pos;
        self.start_drag_and_drop_at(src, x - ox, y - oy);
    }

    /// The manager's drag-start routine, given a source element
    /// and the cursor's offset inside it.
    ///
    /// Walks up from `e` looking for one that is draggable (attribute 0x3A), carrying the grab
    /// offset into each parent's coordinate space (`box.x0 + dx`, `box.y0 + dy`) as the client
    /// does. The found element's **parent** makes the drag proxy,
    /// refusing when the parent carries attribute 0x39. Element message **0x21** carries
    /// the saved drag press point as its window point, which is what
    /// the UI item's message handler hands to
    /// the item list's begin-drag.
    ///
    /// # The walk is a recursion, and its shape is observable.
    ///
    /// The not-draggable arm is: take the parent, then
    /// start drag-and-drop on it at `(box.x0 + dx, box.y0 + dy)`, and **only if that
    /// answers false** the `0x21`. So every level the recursion entered
    /// and did not accept raises its own `0x21` as the stack unwinds — the highest one reached
    /// first and the pressed element **last**, each naming its own element — not one
    /// message, on `e`.
    ///
    /// The draggable arm never raises `0x21` and never walks
    /// further. It asks the parent to drag the item; it returns false the moment that
    /// refuses, and the same when it produced nothing or produced the
    /// source itself — which is what a draggable element with **no** parent does, because the
    /// arm skips the drag-item call entirely and leaves the result at `e`. A 0x39-carrying parent
    /// is not a reason to keep climbing; doing so would let a *different*, higher
    /// element be picked up instead of the drag being refused.
    ///
    /// **Drag-started is set before the walk, not after it** — the client's first two
    /// steps inside the threshold gate clear the drag and set drag-started. That ordering is load-bearing: an item
    /// list picks its icon up from *inside* the 0x21 broadcast, and its own
    /// drag start of the drag icon at `(0x10, 0x10)` is gated on "drag started, or past the
    /// threshold". A rebuild that set the flag only on success would refuse the nested call and no
    /// item could ever be picked up.
    ///
    /// Returns whether a proxy was made.
    pub fn start_drag_and_drop_at(&mut self, e: ElemHandle, dx: i32, dy: i32) -> bool {
        self.drag.started = true;
        let (mut dx, mut dy) = (dx, dy);
        // One entry per recursion level that was entered and did not accept, in call order. The
        // `0x21`s come out of it backwards, which is the unwind.
        let mut entered: Vec<ElemHandle> = Vec::new();
        let mut cur = e;
        loop {
            let dragable = self.node(cur).is_some_and(|n| n.flags.dragable());
            // The drag-item step reads 0x39 off the **parent**, which is the element that makes the
            // proxy.
            let parent = self.parent(cur);
            if dragable {
                // The draggable arm's refusals return at once and raise
                // nothing. A parent that carries 0x39 is not a reason to climb higher, and a
                // draggable element with no parent produces itself and refuses too.
                let no_proxy = parent.is_some_and(|p| {
                    self.node(p).is_some_and(|n| {
                        n.merged_properties()
                            .get_bool(crate::props::attr::NO_DRAG_PROXY)
                            .unwrap_or(false)
                    })
                });
                if no_proxy || parent.is_none() {
                    return false;
                }
                let proxy = self.create_drag_proxy(cur);
                self.drag.element = Some(proxy);
                self.drag.owner = Some(cur);
                self.drag.started = true;
                self.drag.grab = (dx, dy);
                self.broadcast_element_message(cur, msgid::DRAG_PROXY_CREATED, proxy.0, 0);
                // The proxy is made mouse-invisible: a drag icon under the pointer must not be
                // what the hit test finds, or every drop would land on the thing being dragged.
                self.set_mouse_visible(proxy, false);
                let (x, y) = self.mouse.pos;
                self.move_to(proxy, x - dx, y - dy);
                self.bring_to_front(proxy);
                return true;
            }
            entered.push(cur);
            // The null-parent case skips the recursion and goes straight to the `0x21`;
            // otherwise the grab offset moves into the parent's space for the recursive call.
            let Some(p) = parent else { break };
            if let Some(b) = self.node(cur).map(|n| n.region.box_) {
                dx += b.x0;
                dy += b.y0;
            }
            cur = p;
        }
        let (px, py) = self.drag.origin;
        for h in entered.into_iter().rev() {
            let point = self.message_point(h, px, py);
            self.broadcast_element_message_at(h, msgid::DRAG_REJECTED, 0, 0, point);
        }
        false
    }

    /// The element's drag-item plus its match-element.
    ///
    /// Drag-item copies the source's desc and creates a parentless element from the same
    /// layout; match-element then makes the copy *look like* the original — it re-applies every
    /// one of the source's instance properties, resets the media machine from the source's, and
    /// **when the copy has no media of its own and the source carries a live image, clears the
    /// copy's image and takes the source's graphic.**
    ///
    /// That last clause is what makes the drag icon show the item's own
    /// texture: the item list's drag-icon preparation writes the item's icon and its
    /// item id onto the drag icon at run time, and neither is in the desc, so a proxy built from
    /// the desc alone drags an empty square.
    fn create_drag_proxy(&mut self, of: ElemHandle) -> ElemHandle {
        let (layout_did, design, desc, props, image, blit, box_) = match self.node(of) {
            Some(n) => (
                n.layout_did,
                n.layout_design,
                n.desc.clone(),
                n.instance_properties.clone(),
                n.region.image.clone(),
                n.region.blit_mode,
                n.region.box_,
            ),
            None => return self.root(),
        };
        let node = crate::ElementNode::new(
            layout_did,
            design,
            desc,
            Box::new(crate::PlainElement) as Box<dyn crate::Element>,
        );
        let h = self.alloc(node);
        if let Some(n) = self.node_mut(h) {
            n.flags.set_object_is_temporary(true);
            n.flags.set_should_own_object(true);
            n.flags.set_is_root_element(true);
            n.region.object_mode = crate::props::UiObjectMode::ElementSize;
            n.instance_properties = props;
            n.region.image = image;
            n.region.blit_mode = blit;
            n.region.box_ = box_;
        }
        // **Do not `set_parent(h, Some(root))` here.**
        //
        // Retail never re-anchors the proxy. Drag-item builds it as a child element with no
        // parent, whose parent set is handed the parent the element already has — none — and
        // the parent setter's first test is the same-parent early-out, so the
        // parent-size-change update never runs. The drag start
        // then puts the proxy on the screen by bringing it to the top of the root element's
        // children, which is pure list surgery and touches neither the parent nor the box.
        //
        // Going through `set_parent` instead would run the parent-size-change update on an element
        // that has just been flagged is-root-element, and a root's reference boxes are
        // `(the layout's design box -> the live display)`. While the
        // window is the authored 800x600 those two rectangles are identical, `dw = dh = 0`, and
        // the proxy comes out at its design size, so the mistake is invisible. Once Alt+Enter
        // makes the display the monitor, the item
        // tile's four `AnchorStart` edges add `dw` to `x1` and `dh` to `y1`: at 3840x2160 a
        // 32x32 ghost becomes 3072x1592, `move_to` pins its top-left to the cursor, and the UI
        // quad's UVs — which divide the destination extent by the *picture* (`ui_draw::quad`,
        // the client's modulo grid) — run to u = 96, so the icon **tiles** to the
        // bottom-right corner instead of stretching.
        let root = self.root();
        self.bring_child_to_top(root, h);
        h
    }

    /// The drag-and-drop catcher query: if attribute 0x36 is set and 0x38 is
    /// not, this element catches; otherwise ask the parent recursively.
    #[must_use]
    pub fn drag_and_drop_catcher(&self, at: ElemHandle) -> Option<ElemHandle> {
        let mut cur = Some(at);
        while let Some(h) = cur {
            let n = self.node(h)?;
            let p = n.merged_properties();
            let catches = n.drop_catcher
                || p.get_bool(crate::props::attr::DROP_CATCHER)
                    .unwrap_or(false);
            let disabled = p
                .get_bool(crate::props::attr::DROP_DISABLED)
                .unwrap_or(false);
            if catches && !disabled {
                return Some(h);
            }
            cur = n.region.parent;
        }
        None
    }

    /// Stop the drag: build the `DragDropInfo`, ask the target to catch the dropped item,
    /// tell the owning element the drag-and-drop is complete (message **0x15** on failure, **0x16** on
    /// success), then delete the proxy and clear the drag state.
    pub fn stop_drag_and_drop(&mut self) -> Option<DragDropInfo> {
        if !self.drag.started {
            return None;
        }
        // Drag-stop's three state clears run **whatever** happened above:
        // clear the started flag, owning element, and potential source. A drag
        // that started and produced no proxy — the 0x21 path, when no item list picked the icon
        // up — has to clear the flag here or the mouse-move handler swallows every later move.
        let (Some(dragged), Some(owner)) = (self.drag.element, self.drag.owner) else {
            self.drag = DragState::default();
            return None;
        };
        // Drag completion reads `last_drag_cursor_over` **directly** — with no second
        // drop-catcher lookup — because mouse-over processing already stored the catcher
        // there. The walk belongs to the producer; a second one here would be a no-op anyway,
        // because the walk stops on the first catching element, so
        // `catcher(catcher(x)) == catcher(x)`.
        let target = self.drag.last_drag_cursor_over;
        let info = DragDropInfo {
            dragged,
            owner,
            target,
            success: target.is_some(),
        };
        if let Some(t) = target {
            // The first parameter is the **drag-drop record** in the client, not the dragged
            // element: it carries the proxy, the owning element and the catcher. The item list's
            // drop handler checks the catcher's ancestry, then inspects the proxy's icon information.
            // The rebuild also needs the drag's owning element to resolve the source inventory item,
            // so this message puts it in `p2` (a local seam, not retail's layout) — which
            // is otherwise 0 on this message, and is 0 on the owning element's own copy below, so
            // the two broadcasts stay distinguishable.
            self.broadcast_element_message(t, msgid::DROP_FAILED, dragged.0, owner.0);
        }
        let msg = if info.success {
            msgid::DROP_SUCCEEDED
        } else {
            msgid::DROP_FAILED
        };
        self.broadcast_element_message(owner, msg, dragged.0, 0);
        if self
            .node(dragged)
            .is_some_and(|n| n.flags.object_is_temporary())
        {
            self.add_to_delete_queue(dragged);
        }
        self.drag = DragState::default();
        Some(info)
    }
}

#[cfg(test)]
mod text_tag_release_tests {
    use crate::focus::action;
    use crate::text::TextElement;
    use crate::{Delivery, ListenerId, NoticeId, UiSystem};

    /// The manager's internal release payload is the Rust adapter for the retail text element's
    /// pre-base mouse-down-table snapshot. An unmatched release over the same tagged glyph must
    /// not manufacture a link click after the authoritative table says there was no press.
    #[test]
    fn a_tagged_text_release_without_a_matching_press_raises_no_notice() {
        let mut ui = UiSystem::new((200, 100));
        let text = ui.create_hollow(Some(ui.root()));
        ui.put_behaviour(text, Box::<TextElement>::default());
        ui.resize_to(text, 180, 40);
        ui.set_mouse_visible(text, true);
        ui.text_element_mut(text)
            .expect("text behaviour")
            .set_text("<Tell:IIDString:0:Alba>Alba<\\Tell>");
        ui.notices
            .register(NoticeId::IidStringTagClicked, ListenerId::External(1));
        ui.drain_outbox();

        ui.mouse_up(action::PRIMARY_CLICK, 4, 8, false);
        assert!(
            ui.drain_outbox()
                .iter()
                .all(|d| !matches!(d, Delivery::Notice { .. })),
            "no authoritative press means the text-tag click handler does not run"
        );

        ui.mouse_down(action::PRIMARY_CLICK, 4, 8);
        ui.mouse_up(action::PRIMARY_CLICK, 4, 8, false);
        assert!(ui.drain_outbox().iter().any(|d| {
            matches!(
                d,
                Delivery::Notice { id, payload, .. }
                    if *id == NoticeId::IidStringTagClicked
                        && payload.a == 0x1000_0001
                        && payload.text.as_deref() == Some("Alba")
            )
        }));
    }
}

#[cfg(test)]
mod child_action_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::InputEvent;
    use crate::{ElemCtx, ElemHandle, Element, UiSystem};

    /// Every call, in order, as `(who, kind, child_index)` — `kind` is `"child"` for
    /// `on_child_action` and `"own"` for `on_action`.
    type Log = Rc<RefCell<Vec<(&'static str, &'static str, usize)>>>;

    #[derive(Debug)]
    struct Spy {
        name: &'static str,
        log: Log,
        /// The action this element consumes, if any.
        consume: Option<u32>,
        /// The handles, so the log can record *which* element the bubble named as the child.
        order: Vec<ElemHandle>,
    }

    impl Spy {
        fn note(&self, kind: &'static str, child: ElemHandle) {
            let i = self
                .order
                .iter()
                .position(|h| *h == child)
                .unwrap_or(usize::MAX);
            self.log.borrow_mut().push((self.name, kind, i));
        }
    }

    impl Element for Spy {
        fn on_child_action(
            &mut self,
            _ctx: &mut ElemCtx<'_>,
            child: ElemHandle,
            e: &InputEvent,
        ) -> bool {
            self.note("child", child);
            self.consume == Some(e.action)
        }

        fn on_action(&mut self, ctx: &mut ElemCtx<'_>, e: &InputEvent) -> bool {
            let me = ctx.me;
            self.note("own", me);
            self.consume == Some(e.action)
        }
    }

    /// An action is offered to every ancestor before the element that has focus.
    #[test]
    fn an_action_is_offered_to_every_ancestor_before_the_element_that_has_focus() {
        let mut ui = UiSystem::new((800, 600));
        let a = ui.create_hollow(Some(ui.root()));
        let b = ui.create_hollow(Some(a));
        let leaf = ui.create_hollow(Some(b));
        let order = vec![a, b, leaf];
        let log: Log = Rc::default();
        for (h, name) in [(a, "a"), (b, "b"), (leaf, "leaf")] {
            ui.put_behaviour(
                h,
                Box::new(Spy {
                    name,
                    log: Rc::clone(&log),
                    consume: None,
                    order: order.clone(),
                }),
            );
        }
        let e = InputEvent {
            action: 0x25,
            start: true,
            x: 0,
            y: 0,
        };
        assert!(!ui.dispatch_action(leaf, &e), "nobody consumed it");
        assert_eq!(
            *log.borrow(),
            vec![
                // `b` is the nearest ancestor, and it is told the **leaf** raised it…
                ("b", "child", 2),
                // …and so is `a`, one level further up: the child is never re-based.
                ("a", "child", 2),
                // …and only then does the focused element get its own action switch.
                ("leaf", "own", 2),
            ]
        );
    }

    /// The consuming half: an ancestor that returns `true` stops the walk **and** stops the
    /// focused element's own arms running. That is the whole of why `0x25` in the chat entry sends
    /// the line instead of the text element's `0x25` merely relinquishing focus.
    ///
    /// Falsified by ignoring `on_child_action`'s return value in `dispatch_child_action`.
    #[test]
    fn an_ancestor_that_consumes_stops_the_walk_and_the_element_itself() {
        let mut ui = UiSystem::new((800, 600));
        let a = ui.create_hollow(Some(ui.root()));
        let b = ui.create_hollow(Some(a));
        let leaf = ui.create_hollow(Some(b));
        let order = vec![a, b, leaf];
        let log: Log = Rc::default();
        ui.put_behaviour(
            a,
            Box::new(Spy {
                name: "a",
                log: Rc::clone(&log),
                consume: None,
                order: order.clone(),
            }),
        );
        ui.put_behaviour(
            b,
            Box::new(Spy {
                name: "b",
                log: Rc::clone(&log),
                consume: Some(0x25),
                order: order.clone(),
            }),
        );
        ui.put_behaviour(
            leaf,
            Box::new(Spy {
                name: "leaf",
                log: Rc::clone(&log),
                consume: None,
                order,
            }),
        );

        let take = InputEvent {
            action: 0x25,
            start: true,
            x: 0,
            y: 0,
        };
        assert!(ui.dispatch_action(leaf, &take), "b consumed it");
        assert_eq!(
            *log.borrow(),
            vec![("b", "child", 2)],
            "a and the leaf never ran"
        );

        // …and an action `b` does not want goes all the way through to the leaf.
        log.borrow_mut().clear();
        let pass = InputEvent {
            action: 0x16,
            start: true,
            x: 0,
            y: 0,
        };
        assert!(!ui.dispatch_action(leaf, &pass));
        assert_eq!(
            *log.borrow(),
            vec![("b", "child", 2), ("a", "child", 2), ("leaf", "own", 2)]
        );
    }
}
