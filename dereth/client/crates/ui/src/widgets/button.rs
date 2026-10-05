use super::*;

/// The auto-repeat interval used when the element carries neither attribute.
///
/// **The auto-repeat interval is not a constant.** Both periods are read out of the element:
/// the button's mouse-down takes attribute
/// `attr::HOT_CLICK_FIRST_INTERVAL` (`0x10`) for the delay before the first repeat, and its
/// global-message listener takes `attr::HOT_CLICK_REPEAT_INTERVAL` (`0x11`) for every repeat
/// after it. This value is used only for a button that carries neither — which the client's
/// float-attribute read leaves **undefined**, so any fallback at all is more defined than the
/// original.
///
/// **This is a declared deviation and it is unreachable on a scrollbar**, because
/// the scrollbar's default-hot-click setup writes both attributes on the bar
/// and on each of its arrows before any of them can be pressed. It can only be reached by a
/// plain `Button` whose layout sets `0x0F` and neither interval.
pub const FALLBACK_INTERVAL: f64 = 0.1;

/// The seven states the button's state update chooses between.
///
/// These differ from the four base states in [`crate::element::state`].
///
/// ```text
/// update_state:
///     disabled = bool attribute 0x0D
///     if disabled: s = 0x0D
///     else:
///         toggled  = bool attribute 0x0E
///         rollover = bool attribute 0x13
///         base = toggled ? 6 : 1
///         s = base                                            // 1, or 6 when toggled
///         if pressed or (rollover and mouse_over_top): s = base + 1   // 2, or 7
///         if pressed and mouse_over_top:               s = base + 2   // 3, or 8
///     if the layout has a state record for s: set_state(s)
/// ```
///
/// The state-record guard is load-bearing: a button whose layout defines no record for
/// the state it computed **stays where it is** rather than losing its picture.
pub mod state {
    use crate::StateId;
    /// Resting.
    pub const NORMAL: StateId = StateId(1);
    /// Moused over, when attribute 0x13 is set.
    pub const ROLLOVER: StateId = StateId(2);
    /// Held down with the pointer still over it.
    pub const PRESSED: StateId = StateId(3);
    /// Resting, toggled on.
    pub const TOGGLED: StateId = StateId(6);
    /// Moused over, toggled on.
    pub const TOGGLED_ROLLOVER: StateId = StateId(7);
    /// Held down, toggled on.
    pub const TOGGLED_PRESSED: StateId = StateId(8);
    /// Disabled — attribute 0x0D. The meter's "nothing to show" media state is the same id.
    pub const DISABLED: StateId = StateId(0x0D);
}

#[derive(Debug, Default)]
pub struct Button {
    /// The original button inherits text-control behavior; this rebuild composes it explicitly.
    /// That is why a button's caption
    /// is attribute **0x17** on the button itself and not a child element: the caption *is* the
    /// text element's glyph list. The relevant text behavior is delegated below.
    pub text: crate::text::TextElement,
    /// The mouse was pressed on this button and has not been released.
    pub pressed: bool,
    /// Hot-clicking (auto-repeat) is in progress.
    pub hot_clicking: bool,
    /// The time of the next hot click, in the client timer's scale,
    /// which for this crate is [`UiSystem::now`](crate::UiSystem::now).
    ///
    /// **Written by the mouse-down and advanced by the tick.** Without it the tick arm would
    /// fire once per frame, so a held arrow would scroll at the frame rate.
    pub next_hot_click: f64,
    /// Whether this button auto-repeats at all.
    pub hot_click_enabled: bool,
    /// Hot-clicking-in-progress as the mouse-up saw it, carried from 0x1D to 0x19.
    pub suppress_click: bool,
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(Button::default())
}

impl Button {
    /// One of the two float-attribute reads that decide the auto-repeat cadence —
    /// `0x10` before the first repeat, `0x11` before every one after it.
    ///
    /// The client widens the `float` it reads to a `double` before adding, which is what this
    /// cast is.
    ///
    /// **The `unwrap_or` is a declared deviation**: retail's float-attribute read returns
    /// `false` on an absent attribute and leaves its out-parameter undefined, so there is no
    /// faithful answer to transcribe. See [`FALLBACK_INTERVAL`] for why
    /// no scrollbar can reach it.
    fn interval(&self, ctx: &ElemCtx<'_>, id: u32) -> f64 {
        ctx.ui
            .node(ctx.me)
            .and_then(|n| n.merged_properties().get_float(id))
            .map_or(FALLBACK_INTERVAL, f64::from)
    }

    /// The button's state update — the whole of it; see [`state`].
    ///
    /// Called from its post-init, its attribute setter for **0x0D, 0x0E and
    /// 0x13**, its mouse-down, its mouse-up and its mouse-over-top —
    /// every one of the five, because leaving one out leaves a button stuck in the wrong
    /// picture.
    pub fn update_state(&self, ctx: &mut ElemCtx<'_>) {
        use crate::props::attr;
        let Some(n) = ctx.ui.node(ctx.me) else { return };
        let props = n.merged_properties();
        let mouse_over_top = n.region.flags.mouse_over_top;
        let s = if props.get_bool(attr::DISABLED).unwrap_or(false) {
            state::DISABLED
        } else {
            // `(-(toggled != 0) & 5) + 1` — 1 or 6, +1 for rollover, +2 for pressed-over.
            let base = if props.get_bool(attr::TOGGLED).unwrap_or(false) {
                6
            } else {
                1
            };
            let rollover = props.get_bool(attr::ROLLOVER_HIGHLIGHT).unwrap_or(false);
            let mut s = base;
            if self.pressed || (rollover && mouse_over_top) {
                s = base + 1;
            }
            if self.pressed && mouse_over_top {
                s = base + 2;
            }
            StateId(s)
        };
        // Set the state only if the layout has a record for it. State 0 is the
        // element's own base record, which answers none for state 0. The state update never
        // asks for 0, so the guard only ever refuses a state the
        // layout does not define.
        if ctx
            .ui
            .node(ctx.me)
            .is_some_and(|n| n.desc.access_state(s).is_some())
        {
            ctx.ui.queue_set_state(ctx.me, s);
        }
    }

    /// The button's mouse-down gate — **which action is a press at all.**
    ///
    /// Without it the mouse wheel over a radio button toggles it.
    /// It is three tests, easy to misread as two:
    ///
    /// ```text
    ///   is_left = action == 7
    ///   if the element does not want double clicks (flag bit 23) and action == 0x0A:
    ///       treat it as a press and go on to the two tests below
    ///   else if not is_left: return
    ///   if resizing (flag bit 19): return
    ///   if moving (flag bit 18):   return
    /// ```
    ///
    /// So actions **5** and **6** — `DIMOFS_Z[+]` and `DIMOFS_Z[-]`, the two wheel detents —
    /// never set the pressed flag and never reach the state update. The same shape is
    /// on this build's other two kinds that take a press:
    /// the text element's mouse-down (the same comparison, after the
    /// same two bitfield tests) and the list box's mouse-down
    /// (which tests 7 and 0x0A), and the smart-box wrapper's mouse-down switches on 7/8/0x0A
    /// alone. `Scrollbar` and `CheckboxOption` are `Button`s, so this
    /// one gate covers the arrows, the toggles and the radios together rather than
    /// special-casing any of them.
    ///
    /// The wheel still reaches the scrolling ancestor: the element's own mouse-down
    /// broadcasts `0x1C` *before* any of this runs, and the bubble up the parent chain is
    /// what [`crate::scrollable::Scrollable::wheel_target`] consumes.
    fn press_action_is_a_press(&self, ctx: &ElemCtx<'_>, action: u32) -> bool {
        let Some(n) = ctx.ui.node(ctx.me) else {
            return false;
        };
        let dbl = action == crate::focus::DOUBLE_CLICK_ACTIONS[0];
        if action != crate::focus::action::PRIMARY_CLICK && !(dbl && !n.flags.wants_dbl_clicks()) {
            return false;
        }
        // Resizing then moving, in the client's order.
        !(n.flags.is_resizing() || n.flags.is_moving())
    }

    /// The button's mouse-up gate, which is **not** the same predicate as
    /// [`Self::press_action_is_a_press`] and is transcribed separately for that reason:
    ///
    /// ```text
    ///   if not pressed: nothing at all happens
    ///   action == 7    -> in
    ///   action == 0x0A -> in, else nothing happens
    /// ```
    ///
    /// Everything the mouse-up does — the `0x0E` toggle flip, clearing the pressed flag, the
    /// state update and the click itself — sits **inside** that block, so a release carrying
    /// a wheel detent leaves a button exactly as it found it. There is no wants-double-clicks
    /// test here and no moving/resizing test: both spellings of the left button are accepted
    /// outright, which is the note `crate::focus::UiSystem::mouse_up` already carries.
    fn release_action_is_a_click(&self, action: u32) -> bool {
        self.pressed
            && (action == crate::focus::action::PRIMARY_CLICK
                || action == crate::focus::DOUBLE_CLICK_ACTIONS[0])
    }

    /// The button's click handler. It belongs to the button, not to any screen.
    ///
    /// It reads enum attribute `0x12`, refuses a missing value or action 1, then dispatches an
    /// input event with that action, toggle type 3, and its press edge set.
    ///
    /// so the two refusals are **no `0x12` at all** and **`0x12 == 1`**, the client's "do
    /// nothing" action. The three singleton null checks have no counterpart in a build where
    /// the manager is `self`. The manager's action handler's press-edge arm is the key-press
    /// event = [`crate::UiSystem::key_press`], whose tail is
    /// the visibility-toggle action — element message `0x31` on every element
    /// registered under attribute `0x57`, whose `0x58` then shows, hides or toggles it.
    ///
    /// **Re-entrancy:** this runs
    /// with the button's behaviour lifted out of its arena slot, so `key_press`'s
    /// `dispatch_action` on the focus element cannot take it back if the focus element *is*
    /// this button — which it usually is, since a press focuses the button. Nothing is lost: `Button` does not
    /// override `on_action`, the default answers `false`, and `dispatch_child_action` walks
    /// the **parents**, which are not lifted. The retail chain reaches
    /// the text element's action handler, whose own switch covers the text-editing actions
    /// `0x16`–`0x28` and would not consume a panel action either.
    ///
    /// Returns whether the action was fired, which is what decides `StopProcessing` above.
    pub fn handle_button_click(&self, ctx: &mut ElemCtx<'_>) -> bool {
        use crate::props::attr;
        let Some(n) = ctx.ui.node(ctx.me) else {
            return false;
        };
        let Some(action) = n.merged_properties().get_enum(attr::BUTTON_INPUT_ACTION) else {
            return false;
        };
        if action == 1 {
            return false;
        }
        let (x, y) = ctx.ui.mouse_pos();
        ctx.ui.key_press(&crate::focus::InputEvent {
            action,
            start: true,
            x,
            y,
        });
        true
    }
}

impl Element for Button {
    /// **`Button` is always mouse-visible, and that is what makes any button in this
    /// client clickable at all.**
    ///
    /// The original button, menu, drag handle, resize handle, and scrollbar all answer true to
    /// the mouse-visibility query. Nothing else on the shipped
    /// character-management layout is hit-testable: its six buttons carry no tooltip and no
    /// context-menu flag, so the base mouse-visibility query answers false for
    /// every one of them, and no shipped code registers any of the four mouse messages for
    /// their element ids either — the two other routes to mouse visibility.
    ///
    /// Without it initialisation step 5 leaves every button
    /// mouse-invisible, so recursive hit testing could never return one under its
    /// mouse-visible-or-block-clicks predicate.
    fn should_be_mouse_visible(&self) -> bool {
        true
    }

    /// The original button's inherited text/scrollable mouse-down behavior means a press on
    /// **any** button moves the focus element; see
    /// [`crate::element::Element::takes_focus_on_press`].
    ///
    /// **The retail ordering is the opposite of this build's, and the outcome is the same.**
    /// Retail runs the inherited text mouse-down — hence its focus-taking step — **first**, and
    /// only then marks the button pressed and updates its state. So retail's button *is* in
    /// state 1 or 2 when `0x2F` arrives, the default message handler's arm does push it, and
    /// the state update overwrites the result one statement later. This build raises the press
    /// state from the `0x1C` broadcast in step 5 of [`crate::UiSystem::mouse_down`], which is
    /// before step 7's focus take, so the button is already in state 3 and the arm declines
    /// it. Either way no button ends a press in state 4.
    fn takes_focus_on_press(&self) -> bool {
        true
    }

    /// Behavior: the base, then `update_state`.
    fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
        self.update_state(ctx);
    }

    /// The button's state setter — the whole of it, and it is **not** a
    /// pass-through:
    ///
    /// For a toggle button, state 6 sets attribute `0x0E` and state 1 clears it when that
    /// changes the current toggle. It then synchronizes disabled attribute `0x0D` with whether
    /// the requested state is `0x0D`; only a request that changes neither attribute reaches the
    /// base state setter.
    ///
    /// **The `0x0D` arm is how a button whose layout ships `Disabled = true` becomes usable.**
    /// The element's initialisation, step 3, sets the desc's default state, which
    /// for an ordinary button is state 1; the attribute still says disabled, the two disagree,
    /// and this override answers by writing the attribute **false** into the instance
    /// collection and refusing the state. Step 4 then applies the desc's own properties over
    /// the top and `update_state` lands on state 1.
    ///
    /// It is not hypothetical: the shipped `chargen_master` layout `0x21000038` carries
    /// `0x0D = true` on exactly three elements — `0x100003C9` *Help*, `0x100003CA` *Exit* and
    /// `0x100003CB` *Random* — and `CharGenScreen` never enables them. Without this override
    /// all three stay in state `0x0D` and the mouse-up's disabled gate refuses their
    /// click for ever. **In the retail client, clicking the wizard's *Exit* raises
    /// `ID_CharGen_ExitWarning` ("You are about to return to the Main Menu"), so retail's
    /// button is live.**
    ///
    /// This is the rule behind the otherwise surprising behavior that setting state 0 enables
    /// a button.
    ///
    /// **`update_state` has to be called here.** In the
    /// client the bool attribute setter ends in the overridable attribute handler, and
    /// the button's handler's `0x0E`/`0x0D` arms are
    /// the state update. In this crate the behaviour object is **lifted out of its
    /// arena slot** for the duration of this call, so [`crate::UiSystem::on_set_attribute`]'s
    /// `take_behaviour` answers `None` and the subclass half of the dispatch is skipped
    /// entirely — the attribute would change and the button never restate. The visible effect
    /// would be that the char-gen tab strip highlights **the previously selected tab**: every
    /// progress-state change writes `0x0E` on two toggle tabs and neither would restate until
    /// something else happened to call the state update. Calling it directly here is what the
    /// overridable dispatch would have done, and `update_state` queues through
    /// [`crate::UiSystem::queue_set_state`] so it still lands after the lift ends.
    fn set_state(&mut self, ctx: &mut ElemCtx<'_>, s: crate::StateId) -> bool {
        use crate::props::attr;
        let props = match ctx.ui.node(ctx.me) {
            Some(n) => n.merged_properties(),
            None => return false,
        };
        if props.get_bool(attr::TOGGLE_BUTTON).unwrap_or(false) {
            let toggled = props.get_bool(attr::TOGGLED).unwrap_or(false);
            if s == state::TOGGLED && !toggled {
                ctx.ui.set_attribute_bool(ctx.me, attr::TOGGLED, true);
                self.update_state(ctx);
                return true;
            }
            if s == state::NORMAL && toggled {
                ctx.ui.set_attribute_bool(ctx.me, attr::TOGGLED, false);
                self.update_state(ctx);
                return true;
            }
        }
        let disabled = props.get_bool(attr::DISABLED).unwrap_or(false);
        if (s == state::DISABLED) != disabled {
            ctx.ui
                .set_attribute_bool(ctx.me, attr::DISABLED, s == state::DISABLED);
            self.update_state(ctx);
            ctx.ui.update_mouse_visibility(ctx.me);
            return true;
        }
        false
    }

    /// The button attribute handler chains to the text-element base first.
    fn on_set_attribute(
        &mut self,
        ctx: &mut ElemCtx<'_>,
        id: u32,
        v: Option<&crate::PropertyValue>,
    ) {
        use crate::props::attr;
        self.text.on_set_attribute(ctx, id, v);
        match id {
            // 0x0D also re-runs `update_mouse_visibility`.
            attr::DISABLED => {
                self.update_state(ctx);
                ctx.ui.update_mouse_visibility(ctx.me);
            }
            attr::TOGGLED | attr::ROLLOVER_HIGHLIGHT => self.update_state(ctx),
            // If hot-clicking is in progress and the new value is false: stop and unregister.
            attr::HOT_CLICK => {
                self.hot_click_enabled = matches!(v, Some(crate::PropertyValue::Bool(true)));
                if self.hot_clicking && !self.hot_click_enabled {
                    self.hot_clicking = false;
                    ctx.ui.want_tick(ctx.me, false);
                }
            }
            _ => {}
        }
    }

    fn as_text_mut(&mut self) -> Option<&mut crate::text::TextElement> {
        Some(&mut self.text)
    }

    fn compose_text(&self, screen: crate::Box2D) -> Vec<crate::text::PlacedGlyph> {
        self.text.compose_text(screen)
    }

    /// Delegated for the same reason [`Self::compose_text`] is:
    /// the original button, menu, and scrollbar share text-control outline behavior. This
    /// rebuild delegates the same element outline state through composition.
    ///
    /// Without this the outline would be drawn for a plain label and silently not
    /// for a button carrying the same attribute. 16 shipped elements are exactly that case.
    fn text_outline_color(&self) -> Option<u32> {
        self.text.text_outline_color()
    }

    /// Delegated for the same reason the two above are: the selection
    /// belongs to the original shared text behavior, so its color-inversion draw arm applies
    /// to this element too.
    fn selection_boxes(&self, screen: crate::Box2D) -> Vec<crate::Box2D> {
        self.text.selection_boxes(screen)
    }

    /// Delegated with the three above: the draw's caret arm is
    /// `TextElement`'s, so it is this widget's too.
    fn caret(&self, screen: crate::Box2D) -> Option<(crate::Box2D, u32)> {
        self.text.caret(screen)
    }

    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        if m.source != ctx.me {
            return R::Default;
        }
        // The button's element-message handler. Messages from another
        // element and messages other than click 1 use the inherited text handler. A disabled
        // button swallows its own click; an enabled one stops only when its click action fires.
        //
        // Three arms, and **two of the three stop the message**:
        //
        // - disabled (attribute `0x0D`) — `handle_button_click` is **not** called and the
        //   message still stops. A disabled button swallows its own click; no ancestor sees
        //   it. That is the client's own early return of `2`.
        // - an action fired — stop.
        // - no `0x12`, or `0x12 == 1` — fall through to the base, i.e. bubble.
        //
        // **`StopProcessing` is the literal `2`**, pinned as a literal in this crate's button
        // dispatch tests. If the message went on bubbling after the action fired, an ancestor
        // could undo what the action did.
        if m.id == msgid::BUTTON_CLICKED {
            let disabled = ctx
                .ui
                .node(ctx.me)
                .and_then(|n| n.merged_properties().get_bool(crate::props::attr::DISABLED))
                .unwrap_or(false);
            if disabled {
                return R::StopProcessing;
            }
            if self.handle_button_click(ctx) {
                return R::StopProcessing;
            }
            return R::Default;
        }
        if m.id == msgid::MOUSE_PRESS {
            // **The action gate first** — see [`Self::press_action_is_a_press`]:
            // without it a wheel detent would press every button it passed over, and the matching
            // release would flip the `0x0E` of any that carried `0x0B`, which is every
            // `CheckboxOption` on every option page.
            if !self.press_action_is_a_press(ctx, m.p1) {
                return R::Default;
            }
            // The button's mouse-down: the press itself raises **nothing**

            // unless attribute 0x0F (auto-repeat) is set, in which case it raises the *hot*
            // click straight away and arms the repeat.
            self.pressed = true;
            // The mouse-down sets the pressed flag and then calls the state update; the
            // state it lands in is 3 (or 8) while the pointer is still over the button and 2
            // (or 7, or the resting state) once it leaves. It is **not**
            // `element::state::ACTIVE` (5), which is the generic-element enum and is a
            // different table.
            self.update_state(ctx);
            if self.hot_click_enabled {
                self.hot_clicking = true;
                ctx.ui.want_tick(ctx.me, true);
                // Mouse-down's final step adds float attribute
                // `0x10` to the current time to schedule the first hot click.
                //
                // The **first** interval is 0x10 and it is a different attribute from the one
                // every later repeat reads; see [`attr::HOT_CLICK_FIRST_INTERVAL`].
                self.next_hot_click =
                    ctx.ui.now.0 + self.interval(ctx, crate::props::attr::HOT_CLICK_FIRST_INTERVAL);
                ctx.ui
                    .broadcast_element_message(ctx.me, msgid::BUTTON_HOT_CLICK, m.p1, 0);
            }
            return R::DontDoDefault;
        }
        if m.id == msgid::MOUSE_OVER_TOP {
            // The button's mouse-over-top setter stores the flag and updates state, and does
            // nothing else. It is a virtual called by the
            // mouse-over switch, which raises message 0x1B at the same moment; the
            // message is what this crate has, and running `update_state` off it is what makes
            // a rollover-highlight button light up under the pointer. Without it a button
            // stays in state 1 for ever and the whole rollover half of the layout is dead
            // artwork.
            self.update_state(ctx);
            return R::Default;
        }
        if m.id == msgid::MOUSE_RELEASE {
            // The release handler gates its **whole** body on `pressed &&
            // (action == 7 || action == 0x0A)`; see [`Self::release_action_is_a_click`].
            if !self.release_action_is_a_click(m.p1) {
                return R::Default;
            }
            // 0x1D arrives before 0x19, so hot-clicking-in-progress -- which the client reads
            // *inside* its own mouse-up -- has to be carried across the two messages.
            self.suppress_click = self.hot_clicking;
            self.pressed = false;
            self.hot_clicking = false;
            ctx.ui.want_tick(ctx.me, false);
            // The release handler flips the toggle **before** updating state, which is what
            // makes a toggle button change picture on the click that toggles it.
            //
            // **The `!disabled` half matters.** The client's flip happens only while the
            // pointer is over the button and it is not disabled, so a disabled toggle button
            // does not change position under a click. Flipping `0x0E` anyway and only
            // refusing to raise the message would differ exactly where a `set_state(0x0D)`
            // button is clicked — the client's Enter Game and
            // Create are disabled that way — and the visible consequence would be a greyed
            // toggle that silently swaps its 1/6 half while it is unusable.
            //
            // A note on the same test: the click flag is computed *inside* it, so
            // the mouse-up's `(!disabled || click-while-disabled)` can only ever be evaluated
            // with `disabled == false`. Attribute `0x0C UICore_Button_clickWhileDisabled` is
            // read and **cannot change the answer** — a shipped branch that can never fire,
            // kept here as the client has it rather than tidied away.
            let disabled = ctx
                .ui
                .node(ctx.me)
                .and_then(|n| n.merged_properties().get_bool(crate::props::attr::DISABLED))
                .unwrap_or(false);
            if !disabled
                && ctx
                    .ui
                    .node(ctx.me)
                    .and_then(|n| {
                        n.merged_properties()
                            .get_bool(crate::props::attr::TOGGLE_BUTTON)
                    })
                    .unwrap_or(false)
                && ctx
                    .ui
                    .node(ctx.me)
                    .is_some_and(|n| n.region.flags.mouse_over_top)
            {
                let now = ctx
                    .ui
                    .node(ctx.me)
                    .and_then(|n| n.merged_properties().get_bool(crate::props::attr::TOGGLED))
                    .unwrap_or(false);
                ctx.ui
                    .set_attribute_bool(ctx.me, crate::props::attr::TOGGLED, !now);
            }
            self.update_state(ctx);
            return R::DontDoDefault;
        }
        if m.id == msgid::MOUSE_CLICK {
            // The same gate one message later. The click flag is computed **inside** the
            // mouse-up's `action == 7 || action == 0x0A` block, so a wheel detent never raises
            // message 1 however the manager spells the release. This build splits the
            // mouse-up's tail into `0x19`, so the test has to be repeated here; `self.pressed`
            // has already been cleared by the `0x1D` arm above, which is why only the action is
            // asked for.
            if m.p1 != crate::focus::action::PRIMARY_CLICK
                && m.p1 != crate::focus::DOUBLE_CLICK_ACTIONS[0]
            {
                return R::Default;
            }
            // The button's mouse-up:
            //
            // ```text
            // disabled = bool attribute 0x0D
            // click = false
            // if pressed and (action == 7 or action == 10):
            //     if pointer over the button and not disabled:
            //         ...0x0B/0x0E toggle...
            //         click = not hot_clicking
            //     pressed = false; ...
            //     if click: broadcast message 1 with (7, 0)
            // ```
            //
            // **The ordinary click is message 1 with `p1 = 7`, not message 2.** Message 2 is
            // the auto-repeat "hot click", raised on mouse-down when attribute
            // 0x0F is set and then by the message-3 tick. Every screen's element-message
            // listener in the client tests for message `1` from a button, including all six
            // buttons on the character-generation screen, so a widget
            // that broadcasts 2 leaves every screen deaf.
            //
            // `msgid::MOUSE_CLICK` is raised only when the
            // press was on this same element, which is the pressed flag; the disabled
            // gate is the client's own and is why the button refresh's `set_state(0x0D)` is
            // enough to make a button inert.
            let was_hot = std::mem::take(&mut self.suppress_click);
            let disabled = ctx
                .ui
                .node(ctx.me)
                .and_then(|n| n.merged_properties().get_bool(crate::props::attr::DISABLED))
                .unwrap_or(false);
            if !disabled && !was_hot {
                // **Queued, not broadcast.** The client raises this *after* the
                // base has raised `0x1D` and `0x19`; raising it here, inside `0x19`'s own
                // dispatch, would make the nested serial number swallow the rest of `0x19`'s
                // bubble and no ancestor of any button would ever see it. See
                // [`crate::UiSystem::queue_element_message`].
                ctx.ui
                    .queue_element_message(ctx.me, msgid::BUTTON_CLICKED, 7, 0);
            }
            return R::DontDoDefault;
        }
        R::Default
    }

    /// The button's global-message listener — the auto-repeat clock.
    ///
    /// **It is not "broadcast 2 while hot-clicking", i.e. one repeat per
    /// frame.** That would scroll a held arrow at the frame rate: two rows in two frames
    /// where retail moves one, and worse the faster the client runs. Every list in the
    /// client is affected, because every scrollbar's arrows are `Button`s and
    /// the scrollbar's default-hot-click setup gives all of them the same two attributes.
    ///
    /// When the pointer is off the button, a scheduled time strictly before the current time is
    /// parked at the current time. When the pointer is on it, a scheduled time at or before the
    /// current time broadcasts hot-click message 2 and advances by float attribute `0x11`.
    ///
    /// Three things a quick reading of the comparisons would not have settled:
    ///
    /// * **The repeat boundary is inclusive.** The equal case *fires*:
    ///   `next_hot_click <= now`.
    /// * **The park boundary is exclusive.** Equality does not store: `next_hot_click < now`.
    /// * **The advance accumulates from the previous scheduled time**, not from `now`:
    ///   the addition starts from the scheduled time itself. That is what makes the cadence a
    ///   function of elapsed time rather than of when a frame happened to land.
    ///
    /// The first arm is the one that is easy to leave out and it is not cosmetic: while the
    /// pointer is held down but dragged off the arrow, the clock is dragged forward with it,
    /// so re-entering the arrow after two seconds fires **once**, not sixteen times.
    fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
        if id != crate::msg::global::TICK || !self.hot_clicking {
            return;
        }
        let now = ctx.ui.now.0;
        let over_top = ctx
            .ui
            .node(ctx.me)
            .is_some_and(|n| n.region.flags.mouse_over_top);
        if !over_top {
            // Strictly past only, so an exactly-due repeat is not re-stamped.
            if self.next_hot_click < now {
                self.next_hot_click = now;
            }
            return;
        }
        // Due *or* exactly due.
        if self.next_hot_click > now {
            return;
        }
        ctx.ui
            .broadcast_element_message(ctx.me, msgid::BUTTON_HOT_CLICK, 0, 0);
        self.next_hot_click += self.interval(ctx, crate::props::attr::HOT_CLICK_REPEAT_INTERVAL);
    }
}
