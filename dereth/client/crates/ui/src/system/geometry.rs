//! Element placement and display geometry.

use crate::{factory, layout, msg, props, Box2D, ElemHandle, UiSystem};
use dereth_primitives::DataId;

impl UiSystem {
    // ---- geometry -------------------------------------------------------------------------

    /// The region's absolute screen origin.
    #[must_use]
    pub fn screen_origin(&self, h: ElemHandle) -> (i32, i32) {
        let mut x = 0;
        let mut y = 0;
        let mut cur = Some(h);
        while let Some(c) = cur {
            let Some(n) = self.node(c) else { break };
            x += n.region.box_.x0;
            y += n.region.box_.y0;
            cur = n.region.parent;
        }
        (x, y)
    }

    /// The element's box in absolute screen coordinates.
    #[must_use]
    pub fn screen_box(&self, h: ElemHandle) -> Box2D {
        let Some(n) = self.node(h) else {
            return Box2D::empty();
        };
        let (px, py) = self.parent(h).map_or((0, 0), |p| self.screen_origin(p));
        n.region.box_.offset(px, py)
    }

    /// Behavior: this box intersected with every ancestor's.
    /// Returns the empty box `(0,0,-1,-1)` when fully clipped.
    #[must_use]
    pub fn screen_clip_box(&self, h: ElemHandle) -> Box2D {
        let mut clip = self.screen_box(h);
        let mut cur = self.parent(h);
        while let Some(c) = cur {
            if !clip.is_valid() {
                return Box2D::empty();
            }
            clip = clip.intersect(&self.screen_box(c));
            cur = self.parent(c);
        }
        if clip.is_valid() {
            clip
        } else {
            Box2D::empty()
        }
    }

    /// The element's `MoveTo`.
    pub fn move_to(&mut self, h: ElemHandle, x: i32, y: i32) {
        let (x, y) = self
            .node(h)
            .and_then(|n| n.behaviour.as_ref())
            .map_or((x, y), |b| b.constrain_move(self, h, x, y));
        let Some(n) = self.node_mut(h) else { return };
        let b = n.region.box_;
        if b.x0 == x && b.y0 == y {
            let mut out = std::mem::take(&mut self.requests);
            if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
                b.after_move(self, &mut out, h, x, y);
            }
            self.requests = out;
            return;
        }
        n.region.box_ = Box2D::from_xywh(x, y, b.width(), b.height());
        let notify = n.flags.notify_on_move();
        for c in self.children(h) {
            self.update_for_parent_size_change(c);
        }
        if notify {
            self.broadcast_global(msg::global::ELEMENT_GEOMETRY, 0);
        }
        let mut out = std::mem::take(&mut self.requests);
        if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
            b.after_move(self, &mut out, h, x, y);
        }
        self.requests = out;
    }

    /// Resize: clamp to **all four** size attributes,
    /// resize, re-anchor every child, and — if notify-on-resize is set — raise element message 0x24 and
    /// global message 7.
    ///
    /// **All four clamps, each only when its attribute is present** — an absent attribute does
    /// not clamp at all. The order is the client's, so a minimum wins over a maximum that
    /// contradicts it. See [`props::attr::MIN_WIDTH`] for which clamp is which.
    pub fn resize_to(&mut self, h: ElemHandle, w: i32, hgt: i32) {
        let Some(n) = self.node(h) else { return };
        let p = n.merged_properties();
        let clamp = |v: i32, max: u32, min: u32| {
            let mut v = v;
            if let Some(m) = p.get_int(max) {
                v = v.min(m);
            }
            if let Some(m) = p.get_int(min) {
                v = v.max(m);
            }
            v
        };
        let hgt = clamp(hgt, props::attr::MAX_HEIGHT, props::attr::MIN_HEIGHT);
        let w = clamp(w, props::attr::MAX_WIDTH, props::attr::MIN_WIDTH);
        let b = n.region.box_;
        if b.width() == w && b.height() == hgt {
            let mut out = std::mem::take(&mut self.requests);
            if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
                b.after_resize(self, &mut out, h);
            }
            self.requests = out;
            return;
        }
        let notify = n.flags.notify_on_resize();
        if let Some(n) = self.node_mut(h) {
            n.region.box_ = Box2D::from_xywh(b.x0, b.y0, w, hgt);
        }
        for c in self.children(h) {
            self.update_for_parent_size_change(c);
        }
        if notify {
            self.broadcast_element_message(h, msg::element::id::RESIZED, 0, 0);
            self.broadcast_global(msg::global::ELEMENT_GEOMETRY, 0);
        }
        let mut out = std::mem::take(&mut self.requests);
        if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
            b.after_resize(self, &mut out, h);
        }
        self.requests = out;
    }

    /// The text element's resize-to-paper — grow the element to the text it holds.
    ///
    /// It uses authored maximum width `0x3D` or the display width, subtracts the horizontal
    /// margins before laying out the glyphs, then resizes to the measured dimensions plus all
    /// four margins.
    ///
    /// Its two callers in the client are the fit-to-text setter — attribute **`0x29`**'s arm —
    /// and the text-insert path's tail, which resizes to paper when text bit `0x400` is set, so
    /// an element with the attribute re-sizes after **every** text change. `0x0400` is
    /// [`crate::text::TextBits::fit_to_text`], and this function is its consumer.
    ///
    /// What it matters for, measured: the house panel's add-text sets `0x29` on every
    /// row of the House tab, and the row template `0x100001E6` ships `0x3D`/`0x3F` = **280**. Row
    /// 8's sentence wraps to **three** 16-pixel lines at that width; without the resize the row
    /// element stays at the template's 27, and the list box — which stacks rows by
    /// `n.region.box_.height()` — draws one line and clips the rest.
    ///
    /// Returns false only when `h` is not a text element; the client's own success test cannot
    /// fail (see [`crate::text::TextElement::recalculate_to_paper`]).
    pub fn resize_to_paper(&mut self, h: ElemHandle) -> bool {
        let max_width = self
            .node(h)
            .and_then(|n| n.merged_properties().get_int(props::attr::MAX_WIDTH))
            .unwrap_or(self.display.0);
        let Some((w, hgt)) = self
            .text_element_mut(h)
            .map(|t| t.recalculate_to_paper(max_width))
        else {
            return false;
        };
        self.resize_to(h, w, hgt);
        true
    }

    /// The current-UI-object-mode walk, transcribed.
    ///
    /// The reader the element's resize, its object-scale query and its object-scale update
    /// all use, and the one place where "which element's `0xCD`"
    /// is not "this element's": it walks **up** to the nearest self-or-ancestor that actually owns
    /// a UI object and reads *that* element's attribute.
    ///
    /// Starting at the element, the client walks through parents until it finds one whose
    /// should-own-object flag is set. A color picker always yields mode 3; any other owner yields
    /// authored enum attribute `0xCD`, with mode 3 when the attribute is absent. Reaching the top
    /// without an owner leaves the caller's preinitialized mode 3 unchanged.
    ///
    /// **The default is the caller's, not the function's**, and it matters: two of the four exits
    /// leave the output alone. Every caller in the client writes `3` into it first —
    /// the element's resize, its object-scale query and its object-scale update — so a walk that
    /// falls off the top of the tree answers [`props::UiObjectMode::ElementSize`], which is what
    /// this returns. Written as a `const` default here rather than an `Option` because the
    /// original has no third answer.
    ///
    /// The colour-picker test is the element's type-cast query for type `0x10`: the colour picker
    /// answers itself for that id and null otherwise, and the base answers null for every id.
    /// `0x10` is [`factory::ty::COLOR_PICKER`].
    #[must_use]
    pub fn current_ui_object_mode(&self, h: ElemHandle) -> props::UiObjectMode {
        let mut cur = h;
        loop {
            let Some(n) = self.node(cur) else {
                return props::UiObjectMode::ElementSize;
            };
            if n.flags.should_own_object() {
                // The colour-picker cast, then the attribute, then the default -- in that order.
                if n.desc.ty == factory::ty::COLOR_PICKER
                    || n.desc.engine_ty == factory::ty::COLOR_PICKER
                {
                    return props::UiObjectMode::ElementSize;
                }
                return n
                    .merged_properties()
                    .get_enum(props::attr::UI_OBJECT_MODE)
                    .and_then(props::UiObjectMode::from_value)
                    .unwrap_or(props::UiObjectMode::ElementSize);
            }
            // Having no parent (either way the client asks) is the two exits that leave the
            // caller's `3` standing.
            let Some(p) = n.region.parent else {
                return props::UiObjectMode::ElementSize;
            };
            cur = p;
        }
    }

    /// The element's should-own-object setter, transcribed.
    ///
    /// Two writes of flag bit 14, and **the second overrides the first**:
    ///
    /// The setter first writes flag bit 14 from its argument. A false argument finishes there. A
    /// true argument then reads enum attribute `0xCD`; when authored, its nonzero state replaces
    /// the first write, while an absent attribute leaves the argument in place.
    ///
    /// So [`Self::set_should_own_object`]`(true)` on an element authoring `0xCD = 0` leaves the bit **clear** —
    /// which is exactly the case the manager's root-element creation hits on the nine
    /// shipped mode-0 sites, and the reason those roots own no UI surface. The re-read is not a
    /// belt-and-braces duplicate of `on_set_attribute`'s write: the two run in either order
    /// depending on whether the element is a root, and this one is what makes them agree.
    ///
    pub fn set_should_own_object(&mut self, h: ElemHandle, should: bool) {
        let authored = self
            .node(h)
            .and_then(|n| n.merged_properties().get_enum(props::attr::UI_OBJECT_MODE));
        let Some(n) = self.node_mut(h) else { return };
        n.flags.set_should_own_object(should);
        if !should {
            return;
        }
        if let Some(v) = authored {
            let mode =
                props::UiObjectMode::from_value(v).unwrap_or(props::UiObjectMode::ElementSize);
            n.flags.set_should_own_object(mode.owns_object());
            n.region.object_mode = mode;
        }
    }
    /// The element's update-for-parent-size-change.
    ///
    /// The reference boxes: for a **root** element (or one with no parent element) the old box is
    /// the layout's design box and the new box is the live display; otherwise the parent's
    /// original and current positions. The transform runs from the element's
    /// **design** rectangle, and then the mode-0 rule puts the live coordinate back.
    pub fn update_for_parent_size_change(&mut self, h: ElemHandle) {
        let Some(n) = self.node(h) else { return };
        let design = n.original_position();
        let edges = n.desc.edges;
        let live = n.region.box_;
        let has_size_or_init = live.width() != 0 || live.height() != 0 || n.flags.is_initialized();
        let parent = n.region.parent;
        let is_root = n.flags.is_root_element();
        let layout_design = n.layout_design;

        let (old_ref, new_ref) = match parent {
            Some(p) if !is_root => match self.node(p) {
                Some(pn) => (pn.original_position(), pn.current_position()),
                None => (layout_design, self.display_box()),
            },
            _ => (layout_design, self.display_box()),
        };

        let computed = layout::update_size_and_position(design, old_ref, new_ref, edges);
        let final_box = layout::apply_live_mode_zero(computed, live, edges, has_size_or_init);
        self.move_to(h, final_box.x0, final_box.y0);
        self.resize_to(h, final_box.width(), final_box.height());
    }

    /// Behavior: broadcast **global message 5**, then re-query
    /// the display size and resize the root, which cascades through the whole tree.
    ///
    /// **The cursor bracket, which is deliberate and not decoration.** Retail saves
    /// the last cursor id, sets the last-cursor cache to the invalid id (defeating the set-cursor's
    /// own equality check), draws the dirty regions, and then sets the cursor with the saved id,
    /// the last hotspot and make-default `false`.
    ///
    /// A device reset destroys the `HCURSOR` built from the surface, so the cursor has to be
    /// rebuilt — but the set-cursor returns early when the did and both hotspots match the last
    /// cursor, which they would. Clearing the cache first is what makes the re-push
    /// happen at all; skipping it leaves a stale (or destroyed) cursor on screen. The `false`
    /// matters too: this must not overwrite the default cursor with whatever an element had
    /// overridden it to.
    pub fn refresh_event(&mut self, display: (i32, i32)) {
        let saved = self.last_cursor;
        self.last_cursor = None;
        self.broadcast_global(msg::global::REFRESH, 0);
        self.display = display;
        if let Some(n) = self.node_mut(self.root) {
            n.layout_design = Box2D::new(0, 0, display.0 - 1, display.1 - 1);
        }
        let root = self.root;
        self.resize_to(root, display.0, display.1);
        for c in self.children(root) {
            self.update_for_parent_size_change(c);
        }
        // the dirty-region draw has run (this rebuild redraws every frame); re-push.
        if let Some((did, x, y)) = saved {
            self.set_cursor(did, x, y, false);
        }
    }

    /// Take the cursor push [`UiSystem::set_cursor`] let through, if any.
    ///
    /// The client's half. `None` means nothing changed since the last
    /// call — which is the common case, because the last-cursor cache swallows every redundant push.
    pub fn take_pending_cursor(&mut self) -> Option<(DataId, i32, i32)> {
        self.pending_cursor.take()
    }

    /// Take the clipboard write `Copy` or `Cut` let through, if any.
    ///
    /// The client's half, and a **take** rather than a peek:
    /// `SetClipboardData` calls `EmptyClipboard` first and bumps the clipboard sequence number, so
    /// re-sending an undrained value every frame would make this client fight every other
    /// application on the desktop for the clipboard.
    pub fn take_pending_clipboard(&mut self) -> Option<String> {
        self.pending_clipboard.take()
    }
}
