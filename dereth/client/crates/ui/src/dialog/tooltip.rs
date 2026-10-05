//! Tooltips and the hover machine.
//!
//! Tooltip timing, placement, and ownership live here.
//!
//! Tooltips are ordinary elements created on demand and destroyed by the delete queue. The timing
//! is `Misc.TooltipDelay` seconds of *no mouse motion* over the same element, with a **per-element
//! override in property 0x50**, and a tooltip-duration auto-dismiss. Both are visible
//! behaviour.

use dereth_primitives::LocalTime;

use crate::props::attr;
use crate::{ElemHandle, ElementId, UiSystem};

/// The manager's tooltip/hover group.
#[derive(Debug, Clone)]
pub struct TooltipState {
    /// When the mouse last moved.
    pub last_mouse_move_time: f64,
    /// Whether the hover has started.
    pub hover_started: bool,
    /// When the current tooltip went up.
    pub start: f64,
    /// The `Misc.TooltipEnable` user preference.
    pub enable: bool,
    /// The `Misc.TooltipDelay` user preference, in seconds.
    pub delay: f32,
    /// The auto-dismiss.
    ///
    /// The three tooltip settings are
    /// set together in the element manager's constructor: it enables tooltips, sets the delay to
    /// `0.25`, and sets the duration to `10.0`, in that order.
    ///
    /// The per-tooltip duration setter lets a panel raise
    /// it for long item appraisals.
    pub duration: f64,
    /// The element that owns the current tooltip.
    pub owner: Option<ElemHandle>,
    /// The tooltip window on screen, if any.
    pub element: Option<ElemHandle>,
}

impl Default for TooltipState {
    fn default() -> Self {
        Self {
            last_mouse_move_time: 0.0,
            hover_started: false,
            start: 0.0,
            enable: true,
            // The initial value is a quarter of a second, not a half.
            // The element manager's init registers it afterwards
            // as the `Misc.TooltipDelay` preference,
            // which overwrites it the moment a `UserPreferences.ini` supplies `Misc.TooltipDelay`
            // — so this is the no-file value.
            delay: 0.25,
            duration: 10.0,
            owner: None,
            element: None,
        }
    }
}

impl UiSystem {
    /// The manager's per-frame tooltip check, run once per frame from `use_time`.
    ///
    /// While no hover has started and the mouse is in the window: with no mouse capture and an
    /// element last entered, the delay is that element's float `0x50` or else the preference, and
    /// once the mouse has been still for longer than the delay the hover starts. Otherwise, if a
    /// tooltip is up and has been for longer than the duration, it is queued for deletion and the
    /// mouse-over is switched to nothing.
    pub fn check_tooltip(&mut self, now: LocalTime) {
        if !self.tooltip.hover_started && !self.mouse.has_left_window {
            if self.mouse.capture.is_some() {
                return;
            }
            let Some(entered) = self.mouse.last_entered else {
                return;
            };
            let delay = self
                .node(entered)
                .and_then(|n| n.merged_properties().get_float(attr::TOOLTIP_DELAY))
                .unwrap_or(self.tooltip.delay);
            if self.tooltip.last_mouse_move_time + f64::from(delay) < now.0 {
                self.start_hover(now);
            }
        } else if self.tooltip.element.is_some()
            && now.0 - self.tooltip.start > self.tooltip.duration
        {
            if let Some(e) = self.tooltip.element.take() {
                self.add_to_delete_queue(e);
            }
            self.switch_mouse_over(None);
        }
    }

    /// Start hover → the element's mouse-hover handler: the element returns
    /// true if it wants the hover, which latches `hover_started`.
    ///
    /// The gate is **not** `tooltip-on && the element has tooltip text`. The mouse-hover handler
    /// is short and tests two things:
    ///
    /// It tests bit 5 of the element flags and the manager's tooltip-enable preference, returning
    /// false when either is clear; otherwise it starts the tooltip at the mouse with duration 0.
    ///
    /// There is **no text test here**. Whether there is anything to say is
    /// [`UiSystem::start_tooltip_at_mouse`]'s decision, and it falls back to the element's
    /// authored `0x49` when the stored tooltip text is empty — 96 shipped descriptions carry one
    /// and *nothing in the client sets a tooltip for any of them*. A text conjunct would make every one
    /// of those unreachable. No element class overrides the mouse-hover handler.
    pub fn start_hover(&mut self, now: LocalTime) {
        let Some(entered) = self.mouse.last_entered else {
            return;
        };
        let wants = self.node(entered).is_some_and(|n| n.region.flags.tooltip);
        if !wants || !self.tooltip.enable {
            // The hover is still latched: the original sets `hover_started` from the hover handler's
            // return value, and a false return leaves it clear so the check runs again next frame.
            return;
        }
        self.tooltip.hover_started = true;
        self.tooltip.start = now.0;
        self.start_tooltip_at_mouse(entered, 0.0);
    }

    /// Start the tooltip at the mouse.
    ///
    /// ```text
    /// s = stored tooltip text, or authored StringInfo attribute 0x49
    /// if s is not valid                          return nothing
    /// if element-id attribute 0x47 is absent     return nothing
    /// did = data-id attribute 0x48, or invalid
    /// if invalid and a layout exists, use that layout's data id
    /// if did is invalid                          return nothing
    /// start the tooltip with text, owner, element id, layout id, and duration 0
    /// if a tooltip exists and duration is nonzero, set its duration
    /// ```
    ///
    /// **`0x47` is a hard gate, and it is not a font.** Tooltip startup's fourth argument is the
    /// `ElementDesc` **id** of
    /// the tooltip window, and the DataID beside it is the **layout** that window lives in. An
    /// element with perfectly good tooltip text and no `0x47` draws nothing in retail either.
    /// 203 shipped descriptions carry it; all but one name an element of layout `0x21000041`.
    pub fn start_tooltip_at_mouse(&mut self, h: ElemHandle, duration: f64) -> Option<ElemHandle> {
        let text = self.tooltip_string(h)?;
        if text.is_empty() {
            return None;
        }
        let n = self.node(h)?;
        let own_layout = n.layout_did;
        let p = n.merged_properties();
        let elem = ElementId(p.get_enum(attr::TOOLTIP_ELEMENT)?);
        let did = match p.get_data_id(attr::TOOLTIP_LAYOUT) {
            Some(d) if d.0 != 0 => d,
            _ => own_layout,
        };
        if did.0 == 0 {
            return None;
        }
        let e = self.start_tooltip(&text, h, did, elem)?;
        if duration != 0.0 {
            self.set_duration_for_current_tooltip(duration);
        }
        Some(e)
    }

    /// The stored tooltip text, falling back to the element's authored `0x49`.
    #[must_use]
    fn tooltip_string(&self, h: ElemHandle) -> Option<String> {
        let n = self.node(h)?;
        if let Some(t) = &n.tooltip_text {
            if !t.is_empty() {
                return Some(t.clone());
            }
        }
        let si = n
            .merged_properties()
            .get_string_info(attr::TOOLTIP_ENTRY)?
            .clone();
        if let Some(lit) = &si.literal {
            return Some(lit.clone());
        }
        self.resolve_string(si.table_id?, si.string_id?)
    }

    /// The manager's start-tooltip.
    ///
    /// The tooltip is not a hollow box. It is a *root element built from a layout*, and the string
    /// goes into a text-element child the tooltip itself names:
    ///
    /// ```text
    /// queue any existing tooltip for deletion
    /// retain the new owner
    /// load the layout and find the requested element description
    /// create a root element from that description
    /// select the explicit child id or authored attribute 0x4A
    /// find that descendant and require a text element
    /// assign its StringInfo
    /// measure text with margins under the maximum width
    /// resize the tooltip by the measured text delta
    /// recalculate the glyph list
    /// if scrollable text is taller than its box,
    ///     add the overflow to the tooltip height
    /// apply the final tooltip size
    /// show the tooltip at the requested position
    /// ```
    ///
    /// The four windows of layout `0x21000041` are a 30×30 field with four border
    /// children and one text element at `(2,2)-(27,27)` carrying font `0x40000002` (or
    /// `0x40000015`), white, `max_width` 256. **That font and that background are the tooltip's
    /// look**; a tooltip built without the layout is a zero-size invisible box.
    fn start_tooltip(
        &mut self,
        text: &str,
        owner: ElemHandle,
        did: dereth_primitives::DataId,
        elem: ElementId,
    ) -> Option<ElemHandle> {
        if let Some(e) = self.tooltip.element.take() {
            self.add_to_delete_queue(e);
        }
        self.tooltip.owner = Some(owner);
        // Layout loading reaches the database cache through a singleton; this crate's
        // singleton is [`UiSystem::assets`], installed by the host.
        let assets = self.assets.clone()?;
        let tip = self
            .create_root_by_data_id(assets.as_asset_source(), did, elem)
            .ok()?;
        self.initialize_tree(tip);
        let child = self
            .node(tip)?
            .merged_properties()
            .get_enum(attr::TOOLTIP_TEXT_CHILD)?;
        let tx = self.get_child_recursive(tip, ElementId(child))?;
        if self.text_element_mut(tx).is_none() {
            // The text-element cast failed: the named child is not text, and
            // the original abandons the tooltip rather than showing an empty frame.
            self.add_to_delete_queue(tip);
            return None;
        }

        // The max-width sizing flag is flag 0: `inq_size_with_margins` wraps against attribute `0x3D`
        // (`max_width`), falling back to the display width. All four shipped tooltips set it.
        let max_width = self
            .node(tx)?
            .merged_properties()
            .get_int(attr::MAX_WIDTH)
            .unwrap_or_else(|| self.display().0);
        let text_box = self.screen_box(tx);
        let tip_box = self.screen_box(tip);
        let (want_w, want_h) = {
            let t = self.text_element_mut(tx)?;
            t.set_text(text);
            t.inq_size_with_margins(text, max_width)
        };
        let w = tip_box.width() + (want_w - text_box.width());
        let h = tip_box.height() + (want_h - text_box.height());
        self.resize_to(tip, w, h);

        // The glyph-list recalculation re-wraps into the box the resize just gave the child; this build
        // wraps lazily, so the second pass is the same question asked of the new box.
        let text_box = self.screen_box(tx);
        let needed = self
            .node(tx)
            .and_then(|n| n.behaviour.as_ref()?.measured_text_height(text_box));
        let tip_box = self.screen_box(tip);
        let (mut w, mut h) = (tip_box.width(), tip_box.height());
        if let Some(needed) = needed {
            if needed > text_box.height() {
                h += needed - text_box.height();
            }
        }
        w = w.max(1);
        h = h.max(1);
        self.resize_to(tip, w, h);
        self.start_tooltip_at(tip);
        Some(tip)
    }

    /// The manager's adopt-an-element overload — adopt an already-built element.
    ///
    /// ```text
    /// x = min(max(mouse x + 0x20, 0), display width  - tooltip width)
    /// y = min(max(mouse y + 0x20, 0), display height - tooltip height)
    /// mark the element temporary (flag 0x8000), self-owning and a root element
    /// move it to (x, y)
    /// it becomes the current tooltip, started now
    /// ```
    ///
    /// The **+32** is the cursor offset the client uses so the pointer never covers its own
    /// tooltip, and the two `min`s are why a tooltip near the right edge slides left instead of
    /// running off the screen.
    pub fn start_tooltip_at(&mut self, e: ElemHandle) {
        let b = self.screen_box(e);
        let (dw, dh) = self.display();
        let (mx, my) = self.mouse.pos;
        let x = (mx + 0x20).max(0).min(dw - b.width());
        let y = (my + 0x20).max(0).min(dh - b.height());
        if let Some(n) = self.node_mut(e) {
            n.flags.set_object_is_temporary(true);
            n.flags.set_should_own_object(true);
            n.flags.set_is_root_element(true);
        }
        self.move_to(e, x, y);
        self.bring_to_front(e);
        self.tooltip.element = Some(e);
        self.tooltip.start = self.now.0;
    }

    /// The text element's truncation recalculation, tooltip half — **the one generic
    /// tooltip site in the client**, and the reason a clipped label can be read at all.
    ///
    /// ```text
    /// want  = bool attribute 0xD0
    /// avail = one-line ? box width : box height
    /// need  = one-line ? measured width : measured height
    /// if avail < need: if want, set the tooltip to the full text and turn tooltip-on on
    /// else: clear the truncation position; if want, turn tooltip-on off and clear the tooltip
    /// ```
    ///
    /// Setting the tooltip then makes the element **hit-testable**, so a clipped label stops
    /// clicks falling through it and an unclipped one does not — that asymmetry is an
    /// observable of this site, not just the hover.
    ///
    /// **Where this runs is a deviation, and a deliberate one.** The client calls it from
    /// the glyph-list recalculation and the draw start, i.e. during the draw, because
    /// that is where it re-flows the glyph list into its own surface. This build wraps lazily and
    /// its draw pass borrows the tree immutably, so the sweep runs at the top of
    /// [`UiSystem::use_time`] instead — one step before `check_tooltip` reads the flag, which is
    /// the same steady state the client reaches one frame later.
    pub(crate) fn recalculate_truncation_tooltips(&mut self) {
        const WANT: u32 = attr::AUTO_TOOLTIP_TRUNCATED_TEXT;
        for h in self.element_list.clone() {
            let Some(n) = self.node(h) else { continue };
            // Cheap superset filter first: the full merge allocates, and at most 29 shipped
            // descriptions in the whole dat arm this.
            let armed = n.desc.base.properties.get(WANT).is_some()
                || n.instance_properties.get(WANT).is_some()
                || n.desc
                    .states
                    .values()
                    .any(|s| s.properties.get(WANT).is_some());
            if !armed {
                continue;
            }
            if n.merged_properties().get_bool(WANT) != Some(true) {
                continue;
            }
            let screen = self.screen_box(h);
            if !screen.is_valid() {
                continue;
            }
            let Some(t) = self.text_element_mut(h) else {
                continue;
            };
            // The glyph-list recalculation only reaches this when the text's truncate bit is set,
            // which is attribute `0xC7`'s doing. All 29 shipped `0xD0` descriptions also carry `0xC7`.
            if !t.bits.truncate() {
                continue;
            }
            let one_line = t.bits.one_line();
            let full = t.glyphs.inq_text(false);
            let (need_w, need_h) = t.scrollable_extent(screen);
            let truncated = if one_line {
                screen.width() < need_w
            } else {
                screen.height() < need_h
            };
            if truncated {
                if self
                    .node(h)
                    .is_some_and(|n| n.tooltip_text.as_deref() != Some(full.as_str()))
                {
                    self.set_tooltip(h, Some(full));
                }
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.tooltip = true;
                }
            } else {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.tooltip = false;
                }
                if self.node(h).is_some_and(|n| n.tooltip_text.is_some()) {
                    self.clear_tooltip(h);
                }
            }
        }
    }

    /// Stop hover — the element's mouse-unhover handler, and queue the tooltip element for
    /// deletion. Runs from `switch_mouse_over` whenever the pointer leaves the element.
    pub fn stop_hover(&mut self) {
        self.tooltip.hover_started = false;
        self.tooltip.owner = None;
        if let Some(e) = self.tooltip.element.take() {
            self.add_to_delete_queue(e);
        }
    }

    /// Set the tooltip text.
    ///
    /// **Its tail is a mouse-visibility update, and the tail is the interesting half.** After the
    /// text is stored:
    ///
    /// ```c
    /// if this element owns a live tooltip, reset it
    /// if neither authored nor derived visibility applies, clear mouse visibility and return
    /// otherwise set mouse visibility
    /// ```
    ///
    /// and derived mouse visibility is true for a context menu or valid tooltip text. So
    /// **giving an element a tooltip makes it hit-testable and taking the tooltip away makes it
    /// transparent again**, without touching the authored mouse-visible flag.
    ///
    /// That is not a detail: it is the whole reason a click on the radar can select a blip.
    /// The radar sets no mouse-visible attribute anywhere, and the shipped `0x21000005`
    /// layout gives `0x100006D2` none either, so the radar body is normally invisible to the
    /// pointer and clicks fall through it to the game view. The radar's draw sets a
    /// tooltip — the hovered object's name — exactly while a blip is within six pixels of the
    /// cursor, and clears it otherwise. The radar is therefore clickable **only** while
    /// its object-under-mouse id is non-zero, which also guards the select arm of its
    /// element-message handler.
    /// Besides the visibility update there is the `==` edge guard and the
    /// tooltip reset — and the second is the one with an observable:
    ///
    /// Unchanged text does nothing. Changed text is stored, then an existing tooltip is reset when
    /// this element owns it.
    ///
    /// **A tooltip window whose owner's text changes is destroyed on the spot**, and that is the
    /// only thing that takes a tooltip down while the pointer stays inside the same element.
    /// `switch_mouse_over`/`stop_hover` cannot do it: they fire when the pointer *leaves* an element,
    /// and the 3-D viewport is one element covering the whole world — a pointer sliding off a
    /// chest and onto empty ground never leaves `<SBOX>`. Without the reset the chest's name
    /// would stay on screen over empty ground.
    pub fn set_tooltip(&mut self, h: ElemHandle, text: Option<String>) {
        // The string compare first: setting the text it already holds does nothing
        // at all — not the store, not the reset, not the visibility recompute.
        if self.node(h).is_some_and(|n| n.tooltip_text == text) {
            return;
        }
        if let Some(n) = self.node_mut(h) {
            n.tooltip_text = text;
        }
        if self.tooltip.owner == Some(h) && self.tooltip.element.is_some() {
            self.reset_tooltip();
        }
        self.update_mouse_visibility(h);
    }

    /// Clear the tooltip by storing an empty `StringInfo`, then perform the same
    /// mouse-visibility recomputation, which with empty tooltip text normally answers *false*.
    ///
    /// It broadcasts **no** element message. The client's clear operation is nine lines long and
    /// broadcasts nothing; `0x1E` (`MOUSE_MOVE`) in particular would be wrong in kind, since it
    /// is what dragbar and scrollbar-thumb movement uses.
    pub fn clear_tooltip(&mut self, h: ElemHandle) {
        self.set_tooltip(h, None);
    }

    /// The region's tooltip-on setter sets or clears bit 5 (mask `0x20`) of its flags word, the
    /// same bit [`UiSystem::start_hover`]'s element-hover result tests.
    ///
    /// **It is a flag and not the text.** Tooltip assignment stores the text; this decides whether the
    /// delayed hover path is allowed to show it at all. Two sites in the client write it directly
    /// rather than through this setter — the text element's truncation recalculation (see
    /// [`UiSystem::recalculate_truncation_tooltips`]) and
    /// the smart-box wrapper's object-found notice, which sets the bit on one path and clears it
    /// on the other, and which is the 3-D
    /// object tooltip.
    pub fn set_tooltip_on(&mut self, h: ElemHandle, on: bool) {
        if let Some(n) = self.node_mut(h) {
            n.region.flags.tooltip = on;
        }
    }

    /// The tooltip window that is on screen right now, if any.
    /// A host-level test can ask whether one exists without owning the crate.
    #[must_use]
    pub const fn tooltip_element(&self) -> Option<ElemHandle> {
        self.tooltip.element
    }

    /// The tooltip delay in seconds — `0.25` until a
    /// `UserPreferences.ini` supplies `Misc.TooltipDelay`.
    #[must_use]
    pub const fn tooltip_delay(&self) -> f32 {
        self.tooltip.delay
    }

    /// Whether tooltip-on is set on an element — the flag [`UiSystem::set_tooltip_on`] writes
    /// and reads, so a test can ask the question
    /// `check_tooltip` asks without reaching into the node.
    #[must_use]
    pub fn tooltip_on(&self, h: ElemHandle) -> bool {
        self.node(h).is_some_and(|n| n.region.flags.tooltip)
    }

    /// Set the duration of the tooltip that is currently up.
    pub fn set_duration_for_current_tooltip(&mut self, secs: f64) {
        self.tooltip.duration = secs;
    }

    /// Behavior: clears the whole hover state; called on mode switches.
    pub fn reset_tooltip(&mut self) {
        if let Some(e) = self.tooltip.element.take() {
            self.add_to_delete_queue(e);
        }
        self.tooltip.hover_started = false;
        self.tooltip.owner = None;
    }
}
