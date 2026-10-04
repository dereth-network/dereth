//! `Scrollable` (type 10) — the scroll offset, and the only consumer of a scrollbar's
//! messages in the whole client.
//!
//! The `0x0D`/`0x0E`/`0x0F`/`0x10` a scrollbar arrow or track click raises are consumed here.
//! It is not the chat log that consumes them in the client, and it is not `ChatInterface`
//! either: the chat interface's message handler has no arm for any of the four (its
//! whole switch is `1`, `0x12`, `0x1B`, `0x1F`, `0x29`, `0x2A`, `0x2F`), and neither has
//! the main chat window's nor
//! the floaty chat window's. The consumer is the **base class of every
//! text element**: the scrollable's own message handler, which forwards to its
//! scrollbar-message helper.
//!
//! In the original hierarchy, text elements inherit shared scrolling behavior. Chat logs, book
//! pages, text boxes, the inscription box, and lists therefore use one scrolling implementation;
//! this crate preserves that behavior through composition and delegation.
//!
//! # The arithmetic
//!
//! Everything is the scrollable x/y (the offset in **pixels**) against the scrollable width/height
//! (the content's own extent) and the element's own box. The scrollbar's position is the float
//! attribute `0x86` in `0..=1`, and the two directions are exact inverses:
//!
//! ```text
//! the scrollbar-position update:  position    = scrollable_y / (scrollable_height - height)
//! the scrollbar-message helper:   scrollable_y = ftol(position * (scrollable_height - height))
//! ```
//!
//! The second is pinned the same way as the scrollbar's position-to-thumb-origin: by
//! being the first one's round trip in the tests.

use crate::{ElemHandle, ElementId, UiSystem};

/// `Scrollable`'s own attribute ids — property group `0x11`, as its
/// available-properties query declares them.
pub mod attr {
    /// 0x71 — the horizontal scrollbar's element id.
    pub const H_SCROLLBAR: u32 = 0x71;
    /// 0x72 — the vertical scrollbar's element id.
    pub const V_SCROLLBAR: u32 = 0x72;
    /// 0x73 — clamp a `set_scrollable_xy` into the scrollable area.
    /// reads it into a local **initialised to 1**, so an absent attribute clamps.
    pub const CLAMP: u32 = 0x73;
    /// 0x74 — the client's tail: keep the bar's position across a content
    /// resize and re-derive the offset from it, rather than the other way round.
    pub const KEEP_POSITION: u32 = 0x74;
}

/// `Scrollable`'s six fields, verbatim from its constructor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Scrollable {
    /// The horizontal scrollbar's element id.
    pub h_scrollbar: Option<ElementId>,
    /// The vertical scrollbar's element id.
    pub v_scrollbar: Option<ElementId>,
    /// The horizontal scroll offset in pixels.
    pub x: i32,
    /// The vertical scroll offset in pixels.
    pub y: i32,
    /// The content's scrollable width.
    pub width: i32,
    /// The content's scrollable height.
    pub height: i32,
}

/// Truncation towards zero, as C's float-to-long conversion does it.
fn ftol(v: f32) -> i32 {
    dereth_primitives::num::to_i32(v)
}

#[allow(clippy::cast_precision_loss)]
fn f(v: i32) -> f32 {
    v as f32
}

/// The relative-element search — how a scrollable finds a bar that is its own *sibling*
/// rather than its child. The chat log `0x10000011` and its bar `0x10000012` are both children of
/// `0x10000010`, so a recursive child search from the log would never find it.
///
/// 1. Search my own subtree; return any match.
/// 2. Then, for me and each ancestor in turn: if that node has the id, return it; otherwise, if
///    it has a parent, check each of the parent's children — a child with the id is the match,
///    and each child other than the one just come from is searched recursively. Stop climbing
///    after a root element or when there is no parent.
///
/// **It is a nearest-first *outward* search, and that matters.** Walking straight up to the root
/// element and doing one recursive child search from there returns the **first** match in
/// document order. The char-gen wizard has **three** description
/// panes — heritage `0x100003C4`, appearance `0x100003AB`, summary `0x10000404` — and all three
/// name the same bar id `0x100002E7`, because each page is a separate shipped layout that happens
/// to reuse it. Under a root-down search all three would bind the **heritage page's** bar, and
/// the scrollbar-pointer query's identity guard would not catch it: the reverse search (from the bar, for the
/// caller's id) searches from the same root and finds the same caller back, so the guard agrees every time. The
/// visible result would be one bar taking three panes' positions and two bars never touched at all.
///
/// The search stops **after** processing a root element, so a root's siblings are never reached.
fn find_relative(ui: &UiSystem, me: ElemHandle, id: ElementId) -> Option<ElemHandle> {
    if let Some(h) = ui.get_child_recursive(me, id) {
        return Some(h);
    }
    let mut prev = me;
    let mut node = ui.parent(me);
    loop {
        if ui.node(prev).map(crate::ElementNode::element_id) == Some(id) {
            return Some(prev);
        }
        let cur = node?;
        for c in ui.children(cur) {
            if ui.node(c).map(crate::ElementNode::element_id) == Some(id) {
                return Some(c);
            }
            if c != prev {
                if let Some(h) = ui.get_child_recursive(c, id) {
                    return Some(h);
                }
            }
        }
        node = if ui.node(cur).is_some_and(|n| n.flags.is_root_element()) {
            None
        } else {
            ui.parent(cur)
        };
        prev = cur;
    }
}

impl Scrollable {
    /// The scrollbar pointer — the bar named by `0x71`/`0x72`, but **only** when that
    /// bar can find this element back by id, which is the client's own guard against two
    /// scrollables naming one bar.
    #[must_use]
    pub fn scrollbar(&self, ui: &UiSystem, me: ElemHandle, horizontal: bool) -> Option<ElemHandle> {
        let id = if horizontal {
            self.h_scrollbar
        } else {
            self.v_scrollbar
        }?;
        let bar = find_relative(ui, me, id)?;
        let my_id = ui.node(me).map(crate::ElementNode::element_id)?;
        if find_relative(ui, bar, my_id) == Some(me) {
            Some(bar)
        } else {
            None
        }
    }

    /// The element's own width or height along the requested axis.
    fn view(ui: &UiSystem, me: ElemHandle, horizontal: bool) -> i32 {
        let b = ui
            .node(me)
            .map_or_else(crate::Box2D::default, |n| n.region.box_);
        if horizontal {
            b.width()
        } else {
            b.height()
        }
    }

    /// The **travel**: the content's extent less the view's. Zero or less means nothing to scroll.
    fn travel(&self, ui: &UiSystem, me: ElemHandle, horizontal: bool) -> i32 {
        let content = if horizontal { self.width } else { self.height };
        content - Self::view(ui, me, horizontal)
    }

    /// Update the scrollbar position — write this element's offset onto its bar as the
    /// float attribute `0x86`.
    ///
    /// Travel is content extent minus view extent. The bar position is `offset / travel` when
    /// travel's magnitude is at least `0.0002`, otherwise zero. The client suspends layout while
    /// writing position `0x86` and orientation `0x7B`, then updates layout once.
    ///
    /// The `0x75` pair is the client stopping the bar from laying itself out between the two
    /// writes and then doing it once.
    ///
    /// **The final layout update is not optional; without it the thumb goes dead.** Letting each
    /// write lay out on its own is **not** the same picture. The client's
    /// explicit final layout update is the only thing that lays the thumb out when the
    /// bar's own `on_set_attribute` cannot run, and on the path that matters it cannot: an arrow
    /// click reaches here from **inside the bar's own message dispatch**, with the bar's behaviour
    /// lifted out of its arena slot, so `set_attribute_float`'s subclass half is skipped in
    /// silence. Without the final update, a chat-log arrow click scrolls the log while the thumb
    /// stays where it was. See [`crate::widgets::scrollbar::update_layout_of`].
    pub fn update_scrollbar_position(&self, ui: &mut UiSystem, me: ElemHandle, horizontal: bool) {
        let Some(bar) = self.scrollbar(ui, me, horizontal) else {
            return;
        };
        let travel = self.travel(ui, me, horizontal);
        let offset = if horizontal { self.x } else { self.y };
        let pos = if f(travel).abs() >= 0.0002 {
            f(offset) / f(travel)
        } else {
            0.0
        };
        ui.set_attribute_bool(bar, crate::widgets::scrollbar::attr::HORIZONTAL, horizontal);
        ui.set_attribute_float(bar, crate::widgets::scrollbar::attr::POSITION, pos);
        crate::widgets::scrollbar::update_layout_of(ui, bar);
    }

    /// Update the scrollbar size — the thumb's **proportion** (`0x88`), the bar's disabled
    /// flag (`0x76`), and a reset of the offset when the content no longer overflows.
    ///
    /// On the chosen axis (content = scrollable width or height, view = the element's width or
    /// height): if the content fits (`content <= view`) the offset resets to 0, the bar is
    /// disabled and the content is treated as `view`; an offset past the content is clamped to it.
    /// The proportion is `1.0` for zero content, else `min(view / content, 1.0)`. The bar's
    /// position (`0x86`) is read and zeroed when the content fit before. Then, on the bar, `0x75`
    /// is set, `0x7B` gets `horizontal`, `0x76` the disabled flag and `0x88` the proportion, and
    /// `0x75` is cleared. If the bar's `0x74` is set, the position is written back to `0x86`, the
    /// offset becomes `ftol(pos * travel)` and the scroll-offset change is applied; otherwise the
    /// scrollbar position is updated from the offset.
    ///
    /// Returns whether the offset moved, which is what triggers the scroll-offset change.
    pub fn update_scrollbar_size(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        horizontal: bool,
    ) -> bool {
        let Some(bar) = self.scrollbar(ui, me, horizontal) else {
            return false;
        };
        let view = Self::view(ui, me, horizontal);
        let content = if horizontal { self.width } else { self.height };
        let before = if horizontal { self.x } else { self.y };
        let mut offset = before;
        let mut disabled = false;
        let mut extent = content;
        if content <= view {
            offset = 0;
            disabled = true;
            extent = view;
        }
        if extent < offset {
            offset = extent;
        }
        let proportion = if extent == 0 {
            1.0
        } else {
            (f(view) / f(extent)).min(1.0)
        };
        let mut pos = ui
            .node(bar)
            .and_then(|n| {
                n.merged_properties()
                    .get_float(crate::widgets::scrollbar::attr::POSITION)
            })
            .unwrap_or(0.0);
        if content <= view {
            pos = 0.0;
        }
        if horizontal {
            self.x = offset;
        } else {
            self.y = offset;
        }
        ui.set_attribute_bool(bar, crate::widgets::scrollbar::attr::HORIZONTAL, horizontal);
        ui.set_attribute_bool(bar, crate::widgets::scrollbar::attr::DISABLED, disabled);
        ui.set_attribute_float(bar, crate::widgets::scrollbar::attr::PROPORTION, proportion);
        let keep = ui
            .node(bar)
            .and_then(|n| n.merged_properties().get_bool(attr::KEEP_POSITION))
            .unwrap_or(false);
        if keep {
            ui.set_attribute_float(bar, crate::widgets::scrollbar::attr::POSITION, pos);
            // The client's write of bool attribute `0x75 = false` on the bar above this point is what relays
            // the bar out; see [`update_scrollbar_position`]'s note for why that cannot be relied
            // on here. The non-`keep` arm gets it from [`update_scrollbar_position`].
            crate::widgets::scrollbar::update_layout_of(ui, bar);
            let travel = self.travel(ui, me, horizontal);
            let v = ftol(pos * f(travel));
            if horizontal {
                self.x = v;
            } else {
                self.y = v;
            }
        } else {
            self.update_scrollbar_position(ui, me, horizontal);
        }
        let after = if horizontal { self.x } else { self.y };
        after != before
    }

    /// Set the scrollable offset — move it, clamping into `0 ..= content - view` unless
    /// `force` is set or attribute `0x73` says otherwise. Returns whether it moved.
    pub fn set_scrollable_xy(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        mut x: i32,
        mut y: i32,
        force: bool,
    ) -> bool {
        let clamp = ui
            .node(me)
            .and_then(|n| n.merged_properties().get_bool(attr::CLAMP))
            .unwrap_or(true);
        if !force && clamp {
            x = x.min(self.travel(ui, me, true)).max(0);
            y = y.min(self.travel(ui, me, false)).max(0);
        }
        if x == self.x && y == self.y {
            return false;
        }
        self.x = x;
        self.y = y;
        self.update_scrollbar_position(ui, me, true);
        self.update_scrollbar_position(ui, me, false);
        true
    }

    /// Resize the scrollable area — the content's extent changed. Broadcasts `0x32` with
    /// the **old** width and height, as the client does.
    pub fn resize_scrollable_area(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        width: i32,
        height: i32,
    ) -> bool {
        if self.width == width && self.height == height {
            return false;
        }
        #[allow(clippy::cast_sign_loss)]
        let (ow, oh) = (self.width as u32, self.height as u32);
        self.width = width;
        self.height = height;
        let a = self.update_scrollbar_size(ui, me, true);
        let b = self.update_scrollbar_size(ui, me, false);
        ui.broadcast_element_message(me, crate::msg::element::id::SCROLL_OFFSET, ow, oh);
        a || b
    }

    /// The scrollbar-message helper — the arms that make a scrollbar do anything at all.
    ///
    /// * `0x0A` — the bar moved (a drag, or a stop): the offset follows it,
    ///   `offset = ftol(position · travel)`.
    /// * `0x0D`, `0x0E`, `0x0F`, `0x10` (the client's own test is `0xc < id && id < 0x11`) — an
    ///   arrow or a track click: ask the subclass for the step in pixels (its scroll-delta query) and
    ///   add it.
    ///
    /// `delta` is the subclass's answer; see [`crate::text::TextElement::inq_scroll_delta`].
    /// Returns whether the offset moved.
    pub fn handle_scrollbar_message(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        horizontal: bool,
        msg: crate::MessageId,
        delta: i32,
    ) -> bool {
        if msg == crate::msg::element::id::SCROLL_POSITION {
            let Some(bar) = self.scrollbar(ui, me, horizontal) else {
                return false;
            };
            let pos = ui
                .node(bar)
                .and_then(|n| {
                    n.merged_properties()
                        .get_float(crate::widgets::scrollbar::attr::POSITION)
                })
                .unwrap_or(0.0);
            let v = ftol(pos * f(self.travel(ui, me, horizontal)));
            let before = (self.x, self.y);
            if horizontal {
                self.x = v;
            } else {
                self.y = v;
            }
            return before != (self.x, self.y);
        }
        if !Self::is_step_message(msg) {
            return false;
        }
        let (x, y) = if horizontal {
            (self.x + delta, self.y)
        } else {
            (self.x, self.y + delta)
        };
        self.set_scrollable_xy(ui, me, x, y, false)
    }

    /// `0xc < id && id < 0x11` — the two arrows and the two track halves.
    #[must_use]
    pub fn is_step_message(msg: crate::MessageId) -> bool {
        (0x0D..=0x10).contains(&msg.0)
    }

    /// The scrollable's message handler, **second** arm — the mouse
    /// wheel. Returns the bar the click should be handed to, or `None`.
    ///
    /// For mouse-press message `0x1C` with action 5 or 6, it finds the vertical bar and verifies
    /// that the bar points back to this scrollable and is not under the mouse. It then forwards the
    /// action to the bar's wheel handler and stops processing. A failed lookup or check does nothing.
    ///
    /// **This is where the wheel arrives, and it is not an action handler.** A scrollable has no
    /// action handler of its own, so the wheel arrives some other way — as a *mouse press*.
    /// Retail's chain is:
    ///
    /// ```text
    /// WM_MOUSEWHEEL
    ///   -> the input layer's wheel event       one click of action 5 (up) or 6 (down)
    ///   -> the element manager's action handler  actions 5..=15 become a mouse-down
    ///   -> the element's mouse-down            broadcasts element message 0x1C, p1 = action
    ///   -> HERE                                -> the scrollbar's `handle_mouse_wheel`
    ///   -> the bar broadcasts 0x0D / 0x0E
    ///   -> the FIRST arm of this same function     -> `inq_scroll_delta` -> `set_scrollable_xy`
    /// ```
    ///
    /// So the wheel is routed as an ordinary click on the scrollable and then *reflected off its
    /// own scrollbar* as an arrow message. On **this** side every hop of that chain was already
    /// present and wired: `dereth_input`'s `generate_mouse_wheel_event`, the shipped `DefaultMap`
    /// binding of `DIMOFS_Z[+]`/`[-]` to actions 5/6 in input map `0x0A`, the client UI shell's `on_action`'s
    /// `MOUSE_ACTIONS` range, [`crate::UiSystem::mouse_down`], and the scrollbar's arrow arithmetic.
    ///
    /// The shell routes wheel actions through `UiSystem::mouse_down`; this handler reflects
    /// them through the scrollable's vertical bar. This routing describes this implementation.
    /// A scrollbar's own mouse-press handler handles action 7, not wheel actions 5 and 6.
    ///
    /// Three guards, all kept:
    ///
    /// * **Vertical only.** A scrollable with only a horizontal bar does not wheel.
    /// * **The identity round trip**, which is [`Self::scrollbar`]'s (the scrollbar-pointer query's own
    ///   guard against two scrollables naming one bar) — the client spells it out again here.
    /// * **The bar's mouse-over flag is clear.** A wheel click while the pointer is over the scrollbar
    ///   *itself* is ignored, because the bar is a `Button` and the press is already
    ///   going to be its own. Without this the same gesture would scroll twice.
    #[must_use]
    pub fn wheel_target(
        &self,
        ui: &UiSystem,
        me: ElemHandle,
        msg: crate::MessageId,
        action: u32,
    ) -> Option<ElemHandle> {
        if msg != crate::msg::element::id::MOUSE_PRESS {
            return None;
        }
        if action != crate::focus::action::WHEEL_UP && action != crate::focus::action::WHEEL_DOWN {
            return None;
        }
        // A non-zero vertical scrollbar id, the client's own explicit test before it runs the
        // relative-element search. [`Self::scrollbar`] opens with exactly the same `self.v_scrollbar?`, so
        // the condition is written twice here — as it is in the client.
        //
        // **Measured, not assumed: the two shield each other under mutation.** Deleting this line
        // survives; making `scrollbar` fall back to the horizontal id survives; deleting **both**
        // reddens `a_scrollable_with_only_a_horizontal_bar_does_not_wheel`. So the requirement is
        // tested and neither copy is individually falsifiable. The two guards shield each other,
        // giving a reason to mutate
        // pairs when a single mutation survives a guard you believe in.
        self.v_scrollbar?;
        let bar = self.scrollbar(ui, me, false)?;
        if ui.node(bar).is_some_and(|n| n.region.flags.mouse_over) {
            return None;
        }
        Some(bar)
    }

    /// Which of the two bars raised a message, if either. The client's own first test is whether
    /// the source id equals either scrollbar id, followed by the identity check inside the
    /// scrollbar-pointer query.
    #[must_use]
    pub fn message_is_from_my_bar(
        &self,
        ui: &UiSystem,
        me: ElemHandle,
        source_id: ElementId,
        source: ElemHandle,
    ) -> Option<bool> {
        let horizontal = if Some(source_id) == self.h_scrollbar {
            true
        } else if Some(source_id) == self.v_scrollbar {
            false
        } else {
            return None;
        };
        if self.scrollbar(ui, me, horizontal) == Some(source) {
            Some(horizontal)
        } else {
            None
        }
    }
}

/// The glyph-list recalculation, for every dirty text element in a subtree.
///
/// For a text element whose `0x100` dirty bit is set, the client recalculates the glyph list,
/// resizes the scrollable area to the measured dimensions plus margins, reapplies the current
/// scroll position, and clears the dirty bit.
///
/// **Without it a pane's scrollbar is dead.** The scrollable's own post-initialization measures
/// the content once, at tree-build time, when every description pane is empty. A pane whose text
/// is set afterwards would report an extent of **0 x 0** for ever, so the scrollbar-size update
/// would set the bar `disabled` with a full-length thumb and the arrows would have nothing to
/// move — the char-gen wizard's heritage pane `0x100003C4` is the case, a 265 x 450 box holding
/// the whole Aluvian description.
///
/// # Where it runs
///
/// **This is the layout pass, and the original client's place for it is draw start.**
/// Its text controls and related text-bearing widgets recalculate their glyph lists immediately
/// before drawing each visible region.
///
/// So [`UiSystem::draw`] runs this sweep from the root before it composes anything, and no panel
/// has to remember. The one deviation is that it is a **pre-pass over the tree** rather than a
/// call inside the recursion, because `UiSystem::draw_region` takes `&self`; draw start reads and
/// writes only its own element, so a pre-pass reaches the same state the interleaved walk would.
///
/// There is **no** gate on *"names a scrollbar"*; that would be a real narrowing: a one-line
/// entry box that names no bar (such as `0x10000016`) still has an extent, a truncation position
/// and a line table.
///
/// Returns how many elements were re-measured.
pub fn recalculate_dirty_text(ui: &mut UiSystem, root: ElemHandle) -> usize {
    let mut stack = vec![root];
    let mut all = Vec::new();
    while let Some(h) = stack.pop() {
        all.push(h);
        stack.extend(ui.children(h));
    }
    let mut n = 0;
    for h in all {
        let screen = ui.screen_box(h);
        // The text-element lookup is the current composition seam used by buttons, menus, and
        // scrollbars as well as plain text controls.
        let Some(t) = ui.text_element_mut(h) else {
            continue;
        };
        if !t.bits.dirty() {
            continue;
        }
        // The glyph-list recalculation, then the `0x800` arm.
        let (w, hgt) = t.recalculate_layout(screen);
        let mut scroll = t.scroll;
        t.bits.set_dirty(false);
        scroll.resize_scrollable_area(ui, h, w, hgt);
        let (x, y) = (scroll.x, scroll.y);
        scroll.set_scrollable_xy(ui, h, x, y, true);
        if let Some(t) = ui.text_element_mut(h) {
            t.scroll = scroll;
        }
        n += 1;
    }
    n
}
